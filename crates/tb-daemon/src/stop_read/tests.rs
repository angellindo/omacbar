use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

fn packet_pipe(first: &[u8], second: &[u8]) -> (OwnedFd, OwnedFd) {
    let (source, writer) = pipe_with(PipeFlags::DIRECT | PipeFlags::CLOEXEC).unwrap();
    assert_eq!(rustix::io::write(&writer, first).unwrap(), first.len());
    assert_eq!(rustix::io::write(&writer, second).unwrap(), second.len());
    (source, writer)
}

#[test]
fn pending_packets_arrive_without_further_writes_or_eof() {
    let (source, writer) = packet_pipe(b"down", b"up");
    let (stop, cancel) = io::pipe().unwrap();
    let mut reader = StopRead::inherited(source, Some(Arc::new(stop))).unwrap();
    let (send, receive) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut buf = [0; 32];
        for _ in 0..2 {
            let size = reader.read(&mut buf).unwrap();
            send.send(buf[..size].to_vec()).unwrap();
        }
    });
    let first = receive.recv_timeout(Duration::from_secs(10));
    let second = receive.recv_timeout(Duration::from_secs(10));
    drop(cancel);
    worker.join().unwrap();
    drop(writer);
    assert_eq!(first.unwrap(), b"down");
    assert_eq!(second.unwrap(), b"up");
}

#[test]
fn stop_takes_priority_over_a_packet_already_staged_by_the_previous_read() {
    let (source, _writer) = packet_pipe(b"down", b"up");
    let alias = rustix::io::dup(&source).unwrap();
    let (stop, cancel) = io::pipe().unwrap();
    let mut reader = StopRead::inherited(source, Some(Arc::new(stop))).unwrap();
    let mut buf = [0; 32];
    assert_eq!(reader.read(&mut buf).unwrap(), 4);
    assert_eq!(&buf[..4], b"down");

    let mut source_poll = [PollFd::new(&alias, PollFlags::IN)];
    assert_eq!(poll(&mut source_poll, Some(&rustix::event::Timespec::default())).unwrap(), 0);
    drop(cancel);
    assert_eq!(reader.read(&mut buf).unwrap(), 0);
    assert_eq!(&buf[..4], b"down");
}

#[test]
fn short_packet_reads_discard_the_tail_without_losing_the_following_packet() {
    let (source, writer) = packet_pipe(b"long packet", b"up");
    drop(writer);
    let (stop, _cancel) = io::pipe().unwrap();
    let mut reader = StopRead::inherited(source, Some(Arc::new(stop))).unwrap();

    assert_eq!(reader.read(&mut []).unwrap(), 0);
    let mut short = [0; 2];
    assert_eq!(reader.read(&mut short).unwrap(), 2);
    assert_eq!(&short, b"lo");
    let mut large = [0; 32];
    assert_eq!(reader.read(&mut large).unwrap(), 2);
    assert_eq!(&large[..2], b"up");
    assert_eq!(reader.read(&mut large).unwrap(), 0);
}

#[test]
fn a_short_stream_read_keeps_the_rest_for_later_reads() {
    let (source, writer) = pipe_with(PipeFlags::CLOEXEC).unwrap();
    assert_eq!(rustix::io::write(&writer, b"down\nup\n").unwrap(), 8);
    drop(writer);
    let (stop, _cancel) = io::pipe().unwrap();
    let mut reader = StopRead::inherited(source, Some(Arc::new(stop))).unwrap();

    let mut short = [0; 1];
    assert_eq!(reader.read(&mut short).unwrap(), 1);
    assert_eq!(&short, b"d");
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).unwrap();
    assert_eq!(remaining, b"own\nup\n");
}

#[test]
fn stoppable_inherited_input_rejects_a_write_only_descriptor() {
    let (_source, writer) = io::pipe().unwrap();
    let (stop, _cancel) = io::pipe().unwrap();
    assert_eq!(
        StopRead::inherited(writer, Some(Arc::new(stop))).err().unwrap().raw_os_error(),
        Some(Errno::BADF.raw_os_error())
    );
}
