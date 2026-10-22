# Roadmap de Refactorización: Flujo de Plantillas y Copyfitting (Estilo InCopy)

**Objetivo:** Adaptar la arquitectura de *DesignCraft* para soportar un flujo de trabajo asimétrico donde el "Diseñador" define la geometría y estilos rígidos de una plantilla, mientras el "Redactor" (Usuario Final) edita exclusivamente el texto y realiza ajustes finos de copyfitting (tamaño, tracking, kerning) dentro de rangos estrictamente parametrizados.

---

## El Flujo Operativo Simple (4 Pasos Fundamentales)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. DISEÑADOR: PREPARACIÓN DE PLANTILLAS                                     │
│    • Importa IDML o diseña en DesignCraft Desktop.                          │
│    • Fija marcos con layout_locked = true (geometría y posición intocables). │
│    • Asigna nombres/roles a marcos: "Titular", "Bajada", "Cuerpo", "Foto". │
│    • Define estilos y límites de CopyfitPolicy (±1-2 pt, ±20 tracking).     │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. REPOSITORIO: DISPONIBILIDAD DE PLANTILLAS                                │
│    • Catálogo de plantillas en templates/ (.designcraft o .idml).           │
│    • Al abrirse para redacción, se instancia un documento de trabajo nuevo  │
│      sin riesgo de sobreescribir la plantilla maestra original.             │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. REDACTOR: RELLENADO DE FRAMES (INTERFAZ WEB / WASM)                     │
│    • Acceso desde navegador vía apps/designcraft-web (?mode=editor).        │
│    • Modo Redactor estricto: sin herramientas para mover/deformar cajas.    │
│    • Story Editor / Formulario: redacta texto, pega markdown (**INTERTITULO**)│
│    • Coloca imágenes en marcos gráficos con auto-ajuste proporcional.       │
│    • Semáforo de Copyfit en vivo: alerta de desbordamiento (Overset).       │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. DISEÑADOR: AJUSTES FINALES Y EXPORTACIÓN PDF/X                           │
│    • Retoma el documento completo con el contenido ya volcado.              │
│    • Ajustes finos de microtipografía (saltos de línea ópticos, encuadres). │
│    • Preflight final (fuentes, resolución de imágenes, perfiles CMYK).      │
│    • Exportación a PDF/X-4 de alta resolución para imprenta (export.pdf).   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Fase 1: Extensión del Modelo de Datos (`crates/doc`)
*La capa fundacional: definir las políticas y los bloqueos en la estructura serializable.*

* **Paso 1.1: Bloqueos granulares: Container vs Content**
  * Modificar `Item` (en `crates/doc/src/item.rs`) para separar los permisos del marco (el "hueco" en la página) de su contenido:
    * `layout_locked` (bool): Bloquea el *contenedor* (posición, tamaño de la caja geométrica, rotación).
    * `content_locked` (bool): Bloquea el *contenido* (edición de texto, o el re-enlace de fotografías/PDFs).
  * Esto permite que una "Caja de Publicidad" o un "Marco de Foto" sea inamovible (`layout_locked = true`), pero permita importar un PDF o JPG en su interior (`content_locked = false`).
  * Validar que la serialización lo guarde correctamente en el `.designcraft`.
* **Paso 1.2: Definición de `CopyfitPolicy`**
  * Crear la estructura `CopyfitPolicy` que aloje rangos numéricos opcionales (`std::ops::Range<f64>`) para:
    * Modificadores de fuente (`size_flexibility`).
    * Ajustes de espaciado (`tracking_flexibility`, `kerning_allowance`).
    * Desplazamiento base (`baseline_shift_flexibility`).
* **Paso 1.3: Inyección en Estilos y Serialización**
  * Integrar `CopyfitPolicy` dentro de las definiciones base `ParaStyleDef` y `CharStyleDef` (`crates/doc/src/styles.rs`).
  * Asegurar que estos rangos sean serializables (JSON) y puedan ser importados/exportados.

## Fase 2: Reglas de Negocio y Motor de Comandos (`crates/engine`)
*La barrera de seguridad: impedir que un comando viole el diseño o exceda el copyfitting.*

* **Paso 2.1: Implementar el `EditorMode` (Sesión)**
  * Agregar una bandera al estado de sesión del motor que indique si el cliente actual opera en "Modo Redactor" (Editor_Mode) o "Modo Diseño".
* **Paso 2.2: Intercepción de Comandos Geométricos**
  * En comandos que muten el layout (`transform.set`, `frame.create`, `object.delete`, `page.resize`), inyectar una validación temprana: si el modo es "Redactor" o el item tiene `layout_locked`, retornar un error descriptivo (`Result::Err("Layout is locked by template policy")`).
* **Paso 2.3: Validación Paramétrica en Comandos de Texto**
  * En `crates/engine/src/cmd/type.rs` (ej. `type.char` y `type.para`), al recibir un intento de sobre-escribir un atributo (local override), el motor debe:
    1. Resolver el estilo actual de la selección de texto.
    2. Leer la `CopyfitPolicy` asociada.
    3. Si el override propuesto (ej. `size: 38 pt`) excede el rango máximo paramétrico (ej. max `36 pt`), rechazar o hacer *clamp* del valor y emitir un *Warning* al sistema.

## Fase 3: La Interfaz de Usuario Variante (`crates/ui-egui` y `apps/textcraft`)
*La experiencia del usuario: Ocultar el ruido y guiar al redactor.*

* **Paso 3.1: Perfil de Espacio de Trabajo (Workspace) para Redactores**
  * Crear un sub-perfil de interfaz (`Editor Workspace`) que oculta paneles innecesarios: *Pages*, *Layers*, *Align*, *Pathfinder*, *Stroke*, *Effects*.
* **Paso 3.2: Reducción de la Barra de Herramientas (Container vs Content)**
  * Ocultar la herramienta de Selección General (Flecha Negra) o limitar su acción a solo interceptar comandos inofensivos.
  * Mantener habilitada la **Selección Directa (Flecha Blanca / Direct Selection)**, ya que permite escalar y reencuadrar el contenido gráfico (fotos) dentro de un marco, pero debe rechazar interacciones que muten el marco exterior.
  * Habilitar: `Type Tool` (Texto), `Hand Tool` (Mano), y `Zoom`.
* **Paso 3.3: Panel de Control y Enlaces (Publicidad e Imágenes)**
  * Mantener activo el **Links Panel** para que el redactor pueda hacer `Relink` o colocar (`file.place`) PDFs de pauta publicitaria en los espacios reservados, e interactuar con los *Frame Fitting Options* (Llenar marco proporcionalmente) para asegurar que el arte encaje en su "hueco" sin destrozar la maquetación.
  * Modificar los componentes UI de la barra de control (`Character Panel` y `Paragraph Panel`) para que, si el texto seleccionado tiene `CopyfitPolicy`, los *spinners* ajusten sus topes.

## Fase 4: Flujo de Trabajo e Integración (Pruebas)
*Puesta a punto final y empaquetado para el mundo real.*

* **Paso 4.1: UI de Creación de Políticas (Modo Diseñador)**
  * Agregar campos en el diálogo "Paragraph Style Options" para que los diseñadores puedan introducir de forma visual los rangos paramétricos de copyfitting al configurar la plantilla.
* **Paso 4.2: Exportación / Asignación (Linked Stories)**
  * Integrar con el sistema de `Linked stories` actual (`story.placeAndLink`) para permitir exportar el layout como solo lectura, mientras se asigna una historia en particular al redactor.
* **Paso 4.3: Pruebas End-to-End**
* **Paso 4.3: Pruebas End-to-End**
  * Crear un caso de prueba (`cargo xtask test`) donde un script emule a un usuario intentando mutar la geometría y forzar texto a 72pt. Verificar que la validación reaccione según la política bloqueándolo.

## Fase 5: Herramientas Fundamentales del Flujo Editorial
*Lo que el redactor necesita para trabajar cómodo más allá del layout.*

* **Paso 5.1: Potenciar el "Story Editor" (Editor de Artículos) [COMPLETADO]**
  * Los redactores no siempre quieren escribir en la vista final (columnas estrechas o texto sobre fondos oscuros). En el "Modo Redactor", la vista **Story Editor** es la interfaz central en modo galera.
  * Permite ver el texto en formato continuo, con columna izquierda de estilos de párrafo y el texto desbordado claramente resaltado en rojo en el layout job.
  * **Selector Rápido de Historias:** Incorporado dropdown en el encabezado para alternar instantáneamente entre los marcos de la plantilla ("Titular", "Bajada", "Cuerpo") sin necesidad de cerrar el editor ni interactuar con la geometría del lienzo.
* **Paso 5.2: Flujo de Autoría y "Track Changes" (Control de Cambios) [EN PROGRESO]**
  * El motor soporta marcas de cambios (`ChangeMark::Inserted`, `ChangeMark::Deleted`) con autores y comandos `changes.accept` / `changes.reject` integrados en el Story Editor.
* **Paso 5.3: Notas Editoriales (Notes Panel)**
  * Soporte para notas internas (`notes`) ancladas en la historia para comentarios y observaciones que no se exportan al PDF final.
* **Paso 5.4: Feedback de Copyfit Avanzado (Preflight / Info) [COMPLETADO]**
  * **Banner Adhesivo en Story Editor:** Semáforo en vivo en el encabezado:
    * Verde: `✓ Cabe en el marco (N palabras, M caracteres)`.
    * Rojo: `⚠ DESBORDADO (+X líneas / +Y caracteres no caben)`.
  * **Píldora Predictiva en Barra Inferior:** Notificación en tiempo real en la barra de estado principal con acceso con un solo clic a la historia desbordada.

## Fase 6: Formateo Declarativo en Plantillas (Sin Lógica Hardcodeada) [COMPLETADO]
*Principio fundamental: El motor tipográfico es agnóstico; las plantillas definen todas las reglas.*

* **Paso 6.1: Reglas GREP y Estilos Anidados en `cbody` (Cuerpo de Texto) [COMPLETADO]**
  * **Cero lógica hardcodeada en el motor:** `text.replace` / `story.replaceRange` solo inserta texto de manera pura y predecible.
  * **Intertítulos y Destacados:** El diseñador configura en la plantilla un estilo de párrafo `cbody` con reglas GREP (ej. `\*\*([^*]+)\*\*` o `#([^#]+)#`) mapeadas a un estilo de carácter (`Intertitulo`, `Bold`, etc.). El motor soporta grupos de captura (`$1`) y transpila sintaxis estándar InDesign lookaround `(?<=#)[^#]+(?=#)` de forma transparente.
  * **Viñetas y Listas (Bullets):** Formateadas mediante estilos anidados (*Nested Styles* hasta tabulador/espacio) o reglas GREP para viñetas (`(^|\n)•\s*`).
  * **Autonomía total:** Cualquier cambio editorial de sintaxis o diseño se ajusta editando la plantilla (en IDML o DesignCraft), sin requerir cambios de código fuente ni recompilación de la aplicación.

---

## Fase 7: Conectividad y Consumo de API (Flujo Editorial Redacción ➔ Preprensa)
*El puente digital: Conectar el motor con sistemas CMS, DAM y talleres de preprensa.*

* **Paso 7.1: Inyección de Contenido Estructurado (CMS ➔ Plantilla / Inbound API)**
  * **Evolución del modelo de transporte:** Pasar del intercambio exclusivo de archivos binarios completos (`.designcraft`) al consumo de datos puros estructurados (`JSON`).
  * **Mapeo declarativo por nombre de marco:** Nuevo endpoint o comando (`story.injectFields` / `POST /api/documents/:id/inject`) que recibe un payload JSON:
    ```json
    {
      "template": "noticia_tres_columnas",
      "fields": {
        "Titular": "Acuerdo histórico en el congreso",
        "Bajada": "Tras 14 horas de debate se aprueba la reforma...",
        "Cuerpo": "El pleno inició la sesión...",
        "Foto": "https://cdn.periodico.com/img/congreso.jpg"
      }
    }
    ```
  * El motor ubica automáticamente las cajas nombradas, inyecta los textos y descarga/enraíza las imágenes aplicando *Auto-Fit* proporcional sin desarmar la geometría.

* **Paso 7.2: Consumo Outbound desde la Interfaz Web (WASM Fetch)**
  * **Integración con CMS / DAM en el cliente:** Botón de "Importar Nota" en la barra de herramientas o Story Editor.
  * Mediante llamadas `fetch()` asíncronas del navegador, la app web solicita artículos a la API del CMS por ID/slug y los vuelca directamente en la historia activa.
  * Soporte para selección de fotografías desde un DAM (banco de imágenes interno) insertando URLs directamente en los marcos de imagen protegidos.

* **Paso 7.3: Sincronización de Estados y Métricas de Copyfit (Workflow API)**
  * **Ciclo de vida editorial:** La API mantiene y transiciona el estado de la página:
    $$\text{Borrador} \longrightarrow \text{En Redacción} \longrightarrow \text{En Corrección} \longrightarrow \text{Listo para Preprensa}$$
  * **Telemetría de Copyfit en tiempo real:** Al persistir cambios (`POST /api/documents/:id`), el motor reporta metadatos de maquetación:
    * Estado de desbordamiento: `overset: false` / `overflow_lines: 0`.
    * Conteo exacto de palabras, caracteres y líneas para que el CMS alerte si la nota está lista antes de pasar a la siguiente etapa.

* **Paso 7.4: Automatización de Preprensa Desatendida (Headless Preflight y PDF/X-4)**
  * **Generación en segundo plano:** Al marcar una edición como "Aprobada para Cierre", el backend dispara el CLI en modo desatendido:
    ```bash
    designcraft-cli run --doc edicion_hoy.designcraft --export preprensa/salida_alta.pdf
    ```
  * **Preflight Automático en Servidor:** Validación previa estricta (bloquea la exportación si hay texto en overset, imágenes <300 DPI, fuentes no embebidas o colores RGB no convertidos).
  * **Entrega automática a imprenta:** Generación de PDF/X-4 de alta resolución listo para CTP/rotativa y generación de versión ligera (PDF web / PNG) para archivo y previsualización.

