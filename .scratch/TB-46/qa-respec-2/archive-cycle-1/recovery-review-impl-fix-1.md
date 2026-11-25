# TB-46, ciclo de correcciones 1

Candidato `27643ff24dc7515fb42c131219a2b60e53f8e5bd`, re-spec `5a066f4` y sello
`sha256:e7bb6f5acc32356222015d9feabc791153d7c6b89e2d17ea4f59dc59de0d1dd3`.

| Hallazgo de `review-1` | Verificación |
|---|---|
| 1. Autorización de la restauración | `settle` exige y comprueba la expectativa del config y del destino capturado. `run` aún toma el hash mediante `lstat_state` después de que salga el configurador. Una edición posterior al guardado puede dejar TOML válido y conservar la etiqueta `Rehearsal`. Se puede adoptar su hash, pasar la reapertura y restaurar encima. El límite documentado en S0 incluye un intervalo anterior a la comparación final de ADR-0013 y contradice la re-spec. |
| 2. Limpieza confinada | El recorrido de padres usa `O_NOFOLLOW`. La comparación y el `unlink` usan el mismo descriptor. Las pruebas de sustitución del padre y del almacén por enlaces pasaron. |
| 3. Comparación al reemplazar | Config y destino vuelven a compararse tras preparar el temporal. Las dos pruebas de edición durante la preparación pasaron. |
| 4. Manifiesto del servicio | 2.4 incluye la transición a `state: removed` y su evidencia. B → A repone la copia con `cp -a` y verifica bytes y metadatos con `cmp` y `stat`. Las pruebas documentales pasaron. |

El contrato sellado usa un único HOME y un solo guardado por escenario. El binario simulado
registra su hash en un log del fixture, y su salida pública sólo indica que guardó. Para que
`run` pueda verificar el guardado del configurador y del simulado por la misma interfaz, la spec
debe definir un recibo emitido por la operación productora y el fixture debe emitirlo también.
El configurador puede generarlo a partir de los bytes registrados en `ConfigFile::swap` tras el
guardado explícito. `run` debe rechazar un recibo ausente o inválido, emplear ese hash al asentar
el config y tomar la expectativa del destino del enlace de la captura, ya que el guardado normal
reemplaza el enlace de entrada. La prueba `probe-producer-receipt.py` introduce un cambio de
bytes que conserva la etiqueta entre `os.replace` y la salida del productor simulado.

La unidad `tb46-impl-fix1.service` obtuvo `tests.rc=0` con 51 pruebas de ensayo y 12 de runbook.
El gate del candidato y el ensayo con binarios reales se ejecutan en esa misma unidad. El probe
tiene su propio log y usa el wrapper térmico y el lock del ticket. El árbol de producto y el
contrato se dejaron intactos mientras la unidad estaba activa.
