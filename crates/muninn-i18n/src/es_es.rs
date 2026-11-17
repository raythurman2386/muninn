//! The `es-ES` catalog.
//!
//! Spanish (Spain), neutral software register. This exhaustive `match` mirrors
//! [`crate::en_us`] arm for arm and in the same order, so a key added to the
//! catalog fails `cargo check` here until it has Spanish text. Identifiers,
//! keycaps, product names and on-disk tokens stay as written; see
//! `docs/i18n.md` §5.8.
//!
//! The copy is reviewed by a native or fluent speaker before release. The
//! choices that need care are recorded in `tests/spanish.rs` (the invariant
//! allowlist) and in the pull request's terminology notes.

use crate::key::Key;

/// The Spanish text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping every catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Añadir tarjeta",
        Key::Archive => "Archivar",
        Key::Today => "Hoy",
        Key::OverLimit => "Límite superado",
        Key::KeyboardShortcuts => "Atajos de teclado",
        Key::Close => "Cerrar",
        Key::Save => "Guardar",
        Key::Cancel => "Cancelar",
        Key::Clear => "Borrar",
        Key::LinkedNote => "Nota vinculada",
        Key::NoBoards => "Aún no hay tableros en esta bóveda",
        Key::HelpLastLogLines => "últimas líneas del registro:",
        Key::HelpGroupNavigation => "Navegación",
        Key::HelpNavFocusSearch => "Enfocar la búsqueda",
        Key::HelpNavMoveHighlightedResult => "Mover el resultado resaltado",
        Key::HelpNavOpenHighlightedResult => "Abrir el resultado resaltado",
        Key::HelpNavBackInNoteHistory => "Atrás en el historial de notas",
        Key::HelpNavForwardInNoteHistory => "Adelante en el historial de notas",
        Key::HelpNavWriteReadGraphBoardKanban => "Escritura / Lectura / Gráfico / Tablero / Kanban",
        Key::HelpNavFocusMode => "Modo de concentración: ocultar la interfaz y centrar el cursor",
        Key::HelpNavCloseOutermostLayer => "Cerrar la capa más externa",
        Key::HelpNavOpenCommandPalette => "Abrir la paleta de comandos",
        Key::HelpGroupEdit => "Edición",
        Key::HelpEditFindInOpenNote => "Buscar en la nota abierta",
        Key::HelpEditFindInBoard => "Buscar en el tablero",
        Key::HelpEditHideCardsByTextTagDateOrState => {
            "Ocultar tarjetas por texto, etiqueta, fecha o estado"
        }
        Key::HelpEditPerBoardOverrides => "Anulaciones por tablero, un ajuste a la vez",
        Key::HelpEditInsertWikilink => "Insertar un enlace wiki",
        Key::HelpEditNewNote => "Nueva nota",
        Key::HelpEditImportDocument => "Importar un documento como nota",
        Key::HelpEditSaveNow => "Guardar ahora (el guardado automático está activo)",
        Key::HelpEditToggleWriteRead => "Alternar escritura / lectura",
        Key::HelpEditWikilinkCompletion => "Autocompletado de enlaces wiki",
        Key::HelpEditVaultTagCompletion => "Autocompletado de etiquetas de la bóveda",
        Key::HelpEditIndentFourSpaces => "Sangrar cuatro espacios",
        Key::HelpEditPasteRichText => {
            "El texto enriquecido se convierte en Markdown; una URL sobre una selección se convierte en enlace"
        }
        Key::HelpEditRenameSelectedNote => {
            "Cambiar el nombre de la nota seleccionada (en Tablero: el del tablero)"
        }
        Key::HelpEditDeleteSelectedNote => "Eliminar la nota seleccionada",
        Key::HelpGroupKanban => "Kanban",
        Key::HelpKanbanMoveThroughControls => "Moverse por los controles del tablero",
        Key::HelpKanbanMoveBetweenLaneCards => "Moverse entre las tarjetas de la columna enfocada",
        Key::HelpKanbanMoveSelectionAcrossLanes => "Mover la selección entre columnas",
        Key::HelpKanbanToggleCardDone => {
            "Alternar la marca de completado de la tarjeta seleccionada"
        }
        Key::HelpKanbanMoveSelectedCard => "Mover la tarjeta seleccionada",
        Key::HelpKanbanMoveCardToEdgeOrLane => "Moverla arriba, abajo o a otra columna",
        Key::HelpKanbanOpenCardMenu => "Abrir el menú de la tarjeta enfocada",
        Key::HelpKanbanEditCard => "Editar la tarjeta",
        Key::HelpKanbanCloseMenuOrConfirmation => {
            "Cerrar el menú, la confirmación o la barra de conflicto"
        }
        Key::HelpGroupGraph => "Gráfico",
        Key::HelpGraphToggleGraph => "Alternar gráfico",
        Key::HelpGraphToggleGlobalLocal => "Alternar global / local",
        Key::HelpGraphMoveFocusRing => "Mover el anillo de enfoque",
        Key::HelpGraphOpenFocusedNote => "Abrir la nota enfocada",
        Key::HelpGraphZoomInOut => "Acercar / alejar",
        Key::HelpGraphFitVisibleNodes => "Ajustar los nodos visibles",
        Key::HelpGraphToggleIsolate => "Alternar aislamiento",
        Key::HelpGraphLeaveGraph => "Salir del gráfico",
        Key::HelpGroupBoard => "Tablero",
        Key::HelpBoardPanWithDrag => "Desplazar el tablero (o arrastrar con la mano)",
        Key::HelpBoardZoomAtPointer => "Acercar o alejar en el puntero",
        Key::HelpBoardZoomAtCanvasCentre => "Acercar / alejar en el centro del lienzo",
        Key::HelpBoardPanTheBoard => "Desplazar el tablero",
        Key::HelpBoardFitTheDrawing => "Ajustar el dibujo",
        Key::HelpBoardShapeTools => {
            "Herramienta de rectángulo, rombo, elipse, flecha, línea o trazo libre"
        }
        Key::HelpBoardDrawShape => "Dibujar esa forma; la herramienta sigue seleccionada",
        Key::HelpBoardPlaceText => "Colocar texto y escribirlo en el sitio con un clic",
        Key::HelpBoardEditTextLabel => "Escribir o editar su rótulo de texto",
        Key::HelpBoardDropTheTool => "Soltar la herramienta (volver a seleccionar)",
        Key::HelpBoardSelectElementOrToggle => {
            "Seleccionar un elemento o añadirlo o quitarlo de la selección"
        }
        Key::HelpBoardNudgeSelection => "Mover la selección 1 unidad (Shift: 5)",
        Key::HelpBoardDeleteSelection => "Eliminar la selección",
        Key::HelpBoardSelectEveryElement => "Seleccionar todos los elementos",
        Key::HelpBoardLinkToNote => {
            "Vincular los elementos seleccionados a una nota (vacío lo quita)"
        }
        Key::HelpBoardOpenLinkedNote => "Abrir la nota vinculada del elemento seleccionado",
        Key::HelpBoardOpenClickedLinkedNote => {
            "Abrir la nota vinculada del elemento en el que se hace clic (un clic solo selecciona)"
        }
        Key::HelpBoardUndoRedo => "Deshacer / rehacer dibujos, movimientos y eliminaciones",
        Key::HelpBoardSaveDrawing => "Guardar el dibujo del tablero",
        Key::HelpBoardEscapeClearsSelection => {
            "Borrar la selección; si no, soltar la herramienta; si no, salir de Tablero"
        }
        Key::HelpGroupSidebar => "Barra lateral",
        Key::HelpSidebarOpenThisHelp => "Abrir esta Ayuda",
        Key::HelpSidebarOpenHelpWhenNotTyping => "Abrir la Ayuda cuando no se está escribiendo",
        Key::HelpSidebarMoveBetweenPanes => "Moverse entre paneles",
        Key::HelpSidebarMoveThroughAList => "Recorrer una lista",
        Key::HelpSidebarOpenFocusedRow => "Abrir la fila enfocada",
        Key::HelpSidebarNoteAndFolderActions => "Acciones de nota y de carpeta",
        Key::HelpSidebarSettings => {
            "Ajustes (bóvedas, tema, efectos del gráfico, valores predeterminados de Kanban)"
        }
        Key::SettingsTabVaults => "Bóvedas",
        Key::SettingsTabEditor => "Editor",
        Key::SettingsTabAppearance => "Apariencia",
        Key::SettingsTabGraph => "Gráfico",
        Key::SettingsTabSync => "Sincronización",
        Key::SettingsTabKanban => "Kanban",
        Key::SettingsVaultsOpenTitle => "Abrir una bóveda",
        Key::SettingsVaultsOpenCaption => {
            "Pega una ruta de carpeta o crea una bóveda nueva en una carpeta nueva."
        }
        Key::SettingsVaultsRecentTitle => "Bóvedas recientes",
        Key::SettingsVaultsRecentCaption => {
            "Bóvedas que has abierto recientemente, primero las más recientes."
        }
        Key::SettingsVaultsRecentEmpty => "Aún no hay bóvedas recientes",
        Key::SettingsVaultsPathLabel => "Ruta de la carpeta de la bóveda",
        Key::SettingsVaultsPathHint => {
            "Pega una ruta de carpeta; ~ se expande a tu carpeta personal."
        }
        Key::SettingsVaultsOpenAction => "Abrir bóveda",
        Key::SettingsVaultsNewAction => "Nueva bóveda",
        Key::SettingsEditorAutosaveTitle => "Retardo del guardado automático",
        Key::SettingsEditorAutosaveCaption => {
            "Segundos de inactividad antes de guardar la nota abierta."
        }
        Key::SettingsEditorTemplateTitle => "Plantilla de nota predeterminada",
        Key::SettingsEditorTemplateCaption => {
            "Markdown insertado al principio de cada nota nueva. Déjalo vacío para un encabezado simple."
        }
        Key::SettingsEditorTemplateLabel => "Plantilla de nota predeterminada",
        Key::SettingsEditorTemplatePlaceholder => {
            "Markdown insertado al principio de cada nota nueva"
        }
        Key::SettingsAppearanceThemeTitle => "Tema",
        Key::SettingsAppearanceThemeFollowing => "Siguiendo el tema del sistema de Omarchy",
        Key::SettingsAppearanceLanguageTitle => "Idioma",
        Key::SettingsAppearanceLanguageCaption => {
            "La opción Sistema sigue el idioma de tu sistema operativo y recurre al inglés cuando ningún paquete de idioma coincide."
        }
        Key::SettingsAppearanceLanguageSystem => "Sistema",
        Key::SettingsAppearanceUnavailable => "Aún no está disponible en macOS.",
        Key::SettingsAppearanceStateOn => "Activado.",
        Key::SettingsAppearanceStateOff => "Desactivado.",
        Key::SettingsAppearanceTrayTitle => "Icono de la bandeja del sistema",
        Key::SettingsAppearanceTrayCaption => {
            "Un icono de bandeja con Mostrar Muninn, Nueva nota, Sincronizar ahora, el estado de sincronización y Salir."
        }
        Key::SettingsAppearanceTrayShow => "Mostrar el icono de la bandeja",
        Key::SettingsAppearanceTrayStatusOn => {
            "Activado. Linux lo muestra si se ejecuta un servicio de bandeja; GNOME necesita una extensión AppIndicator."
        }
        Key::SettingsAppearanceCloseToTrayTitle => "Cerrar a la bandeja",
        Key::SettingsAppearanceCloseToTrayNeedsTray => {
            "Necesita el icono de la bandeja. Cerrar la ventana sale de Muninn."
        }
        Key::SettingsAppearanceCloseToTrayOff => "Desactivado. Cerrar la ventana sale de Muninn.",
        Key::SettingsAppearanceCloseToTrayOn => {
            "Activado. Cerrar la ventana deja Muninn en la bandeja; Salir de Muninn desde allí lo cierra. Si ningún servicio de bandeja muestra el icono, deja esta opción desactivada."
        }
        Key::SettingsAppearanceCloseToTrayNoIcon => {
            "Activado, pero ahora mismo no hay icono de bandeja, así que cerrar la ventana sale de Muninn."
        }
        Key::SettingsAppearanceStartupTitle => "Inicio y atajo",
        Key::SettingsAppearanceLoginTitle => "Iniciar Muninn al iniciar sesión",
        Key::SettingsAppearanceLoginOn => "Activado. Muninn se abre al iniciar sesión.",
        Key::SettingsAppearanceShortcutRegistering => "Registrando…",
        Key::SettingsGraphReduceMotionTitle => "Reducir el movimiento",
        Key::SettingsGraphReduceMotionCaption => {
            "Congela el diseño del gráfico, omite los desplazamientos de cámara y deja estáticos los efectos de puntero, selección y gráfico. La opción Sistema sigue la configuración de tu sistema operativo."
        }
        Key::SettingsGraphReduceMotionEnv => {
            "Ahora activado: MUNINN_REDUCED_MOTION está definida, lo que anula esta opción."
        }
        Key::SettingsGraphReduceMotionSystemOn => {
            "Ahora activado, siguiendo la configuración del sistema."
        }
        Key::SettingsGraphReduceMotionSystemOff => {
            "Ahora desactivado, siguiendo la configuración del sistema."
        }
        Key::SettingsGraphReduceMotionOn => "Ahora activado.",
        Key::SettingsGraphReduceMotionOff => "Ahora desactivado.",
        Key::SettingsGraphEffectsTitle => "Efectos del gráfico",
        Key::SettingsGraphEffectsCaption => {
            "La misma familia de efectos dibuja el borde de la selección y las líneas que toca: Clásico mantiene las líneas simples de siempre, Eléctrico crea arcos en el borde y en las líneas y Desactivado dibuja líneas simples sin capas de efecto. Reducir el movimiento congela la opción que elijas."
        }
        Key::SettingsGraphIntensityTitle => "Intensidad del efecto",
        Key::SettingsGraphIntensityCaption => {
            "Escala los arcos de Eléctrico y el quemado al eliminar. Equilibrado es el aspecto original. Clásico no dibuja arcos y Desactivado no dibuja nada, así que la elección de arcos solo se ve con Eléctrico; el quemado se sigue escalando en ambos casos. Reducir el movimiento también congela el resultado."
        }
        Key::SyncHeadingProvider => "Proveedor de sincronización",
        Key::SyncHeadingProviderCaption => {
            "Solo sincronización manual con Sincronizar ahora. Un proveedor para esta bóveda. Desconectar quita las credenciales, no las notas."
        }
        Key::SyncProviderGithubName => "GitHub",
        Key::SyncProviderGithubDescription => {
            "GitHub seleccionado. Usa un repositorio y una rama dedicados, no el repositorio de código de Muninn."
        }
        Key::SyncProviderDriveName => "Google Drive",
        Key::SyncProviderDriveShortName => "Drive",
        Key::SyncProviderDriveDescription => {
            "Google Drive seleccionado. La sincronización mantiene un historial de confirmaciones gestionado por la aplicación en los datos privados de Muninn en tu Drive, no archivos Markdown editables."
        }
        Key::SyncProviderDriveCaption => {
            "Se guarda en los datos privados de Muninn en tu Drive, no en una carpeta visible. Usa el mismo nombre de almacén en cada dispositivo."
        }
        Key::SyncProviderTailscaleName => "Tailscale",
        Key::SyncProviderTailscaleDescription => {
            "Tailscale seleccionado. Un dispositivo aloja un almacén de confirmaciones; los demás se unen a él por nombre MagicDNS a través de tu tailnet."
        }
        Key::SyncFieldGithubOwner => "Propietario de GitHub",
        Key::SyncFieldRepository => "Repositorio",
        Key::SyncFieldBranch => "Rama",
        Key::SyncFieldPrefix => "Prefijo",
        Key::SyncFieldGithubClientId => "ID de cliente de GitHub (opcional)",
        Key::SyncFieldStoreName => "Nombre del almacén",
        Key::SyncFieldGoogleClientId => "ID de cliente de Google (opcional)",
        Key::SyncFieldLoopbackPort => "Puerto de bucle local",
        Key::SyncFieldAllowedLogins => {
            "Inicios de sesión de Tailscale permitidos (opcional, separados por comas)"
        }
        Key::SyncFieldMagicDnsName => "Nombre MagicDNS del host (device.tailnet.ts.net)",
        Key::SyncFieldSharedKey => "Clave compartida del host",
        Key::SyncActionConnect => "Conectar",
        Key::SyncActionSyncNow => "Sincronizar ahora",
        Key::SyncActionDisconnect => "Desconectar",
        Key::SyncTailnetRoleLabel => "Rol de Tailscale",
        Key::SyncTailnetRoleHost => "Alojar aquí",
        Key::SyncTailnetRoleJoin => "Unirse a un host",
        Key::SyncTailnetKeyLabel => "Clave compartida",
        Key::SyncTailnetKeyCopy => "Copiar clave",
        Key::SyncTailnetKeyShow => "Mostrar clave",
        Key::SyncTailnetKeyHide => "Ocultar clave",
        Key::SyncTailnetKeyRotate => "Rotar clave",
        Key::SyncStatusNotConnected => {
            "Sin conexión. Elige un proveedor y un remoto explícito, y luego pulsa Conectar."
        }
        Key::SyncStatusTailnetHost => {
            "Este dispositivo aloja. Muninn sirve el almacén de confirmaciones en 127.0.0.1 mientras está abierto; tailscale serve lo publica en tu tailnet."
        }
        Key::SyncStatusTailnetJoin => {
            "Únete a un host. Escribe el nombre MagicDNS completo del host y pega la clave que muestra Copiar clave en el host."
        }
        Key::SyncStatusConnectedSharedKey => {
            "Conectado. La clave compartida está en el almacén de credenciales del sistema operativo. Pulsa Sincronizar ahora cuando quieras."
        }
        Key::SyncStatusConnectedToken => {
            "Conectado. El token está en el almacén de credenciales del sistema operativo. Pulsa Sincronizar ahora cuando quieras."
        }
        Key::SyncStatusAskingGithubCode => "Solicitando a GitHub un código de inicio de sesión…",
        Key::SyncStatusOpeningGoogle => "Abriendo el inicio de sesión de Google en tu navegador…",
        Key::SyncStatusWaitingGoogle => {
            "Termina de iniciar sesión en Google en tu navegador. Muninn espera la respuesta en 127.0.0.1."
        }
        Key::SyncStatusSignedInAfterDisconnect => {
            "El inicio de sesión terminó después de Desconectar; su token se eliminó."
        }
        Key::SyncStatusTailnetJoined => {
            "Conectado al host. La clave compartida está en el almacén de credenciales del sistema operativo. Sincronizar ahora llega al host a través de tu tailnet."
        }
        Key::SyncStatusSyncStarted => "Sincronización iniciada. La ventana sigue respondiendo.",
        Key::SyncStatusCancelling => {
            "Cancelando. Un cambio que el proveedor ya aceptó se conserva y se reconcilia."
        }
        Key::SyncStatusNoteChangedWhileSyncing => {
            "La nota abierta cambió durante la sincronización; su texto no se sobrescribió."
        }
        Key::SyncStatusNothingConnected => "No hay nada conectado. Las notas locales no cambiaron.",
        Key::SyncStatusDisconnected => {
            "Desconectado. Se eliminaron las credenciales de esta vinculación. Las notas locales y remotas no se borraron."
        }
        Key::SyncStatusSaveBeforeSync => {
            "Guarda o resuelve la nota abierta antes de Sincronizar ahora. El texto sin guardar no se sube ni se sobrescribe."
        }
        Key::SyncStatusAlreadyRunning => "Ya hay una sincronización en curso.",
        Key::SyncStatusFinishSignIn => {
            "Termina primero de iniciar sesión o pulsa Desconectar para cancelar."
        }
        Key::SyncStatusConnectProvider => {
            "Conecta un proveedor para esta bóveda antes de Sincronizar ahora."
        }
        Key::SyncStatusWaitForRun => {
            "Espera a que termine la sincronización o el inicio de sesión en curso, o pulsa Desconectar para cancelarlo."
        }
        Key::SyncStatusWaitToRotateKey => {
            "Espera a que termine la sincronización en curso antes de rotar la clave."
        }
        Key::SyncStatusKeyRotated => {
            "Clave rotada; la clave anterior ya no funciona. En cada dispositivo unido, pulsa Desconectar y vuelve a unirte con la clave nueva (Copiar clave)."
        }
        Key::SyncStatusCurrentConnectionKept => "La conexión actual no cambió.",
        Key::SyncErrorVaultNotCanonical => {
            "Esta carpeta de bóveda no se puede canonicalizar, así que no se puede vincular."
        }
        Key::SyncErrorGithubScope => {
            "Escribe el propietario y el repositorio de GitHub antes de conectar."
        }
        Key::SyncErrorLoopbackPort => {
            "Escribe un puerto de bucle local entre 1024 y 65535 para el host."
        }
        Key::SyncErrorPasteSharedKey => {
            "Pega la clave compartida que se muestra en el host. Se guarda en el almacén de credenciales del sistema operativo, no en los ajustes."
        }
        Key::SyncErrorNotTailnetHost => "Esta bóveda no aloja un almacén de Tailscale.",
        Key::SyncErrorSharedKeyMissing => {
            "Falta la clave compartida en el almacén de credenciales."
        }
        Key::SyncErrorTailnetKeyMissing => {
            "Falta la clave compartida de Tailscale en el almacén de credenciales. Desconecta y vuelve a conectar."
        }
        Key::SyncErrorCancelled => "Sincronización cancelada",
        Key::SyncErrorTimedOutAfterSend => {
            "Se agotó el tiempo tras enviar un cambio; reconcilia antes de reintentar"
        }
        Key::SyncErrorTimedOutBeforeResponse => {
            "Se agotó el tiempo antes de que respondiera el proveedor"
        }
        Key::SyncErrorRateLimited => "Demasiadas peticiones",
        Key::SyncErrorLeaseBusy => "Otra instancia de Muninn está escribiendo en esta bóveda",
        Key::SyncErrorSettingsNotObject => {
            "Los ajustes de sincronización no eran un objeto, así que la sincronización está desactivada."
        }
        Key::SyncErrorSettingsVersion => {
            "La versión de los ajustes de sincronización no es compatible, así que la sincronización está desactivada."
        }
        Key::SyncErrorSettingsVaultList => {
            "Faltaba la lista de bóvedas de sincronización, así que la sincronización está desactivada."
        }
        Key::SyncErrorSettingsBindingInvalid => {
            "Una vinculación de sincronización no era válida y no se cargó. La sincronización está desactivada hasta que se reemplace."
        }
        Key::KanbanUiNotInsideVault => "el kanban no está dentro de la bóveda abierta",
        Key::KanbanUiThis => "el kanban",
        Key::KanbanUiFieldRefused => {
            "No se sale: el kanban no se pudo guardar. Corrige lo que impidió el guardado o deshaz los cambios y vuelve a salir."
        }
        Key::ErrorSaveKanbanBeforeRestore => {
            "Guarda o descarta el kanban actual antes de restaurar otro"
        }
        Key::PaletteTitle => "Paleta de comandos",
        Key::PaletteEmpty => "Ninguna nota ni acción coincide",
        Key::PaletteFooter => "↑↓ mover · Enter abrir · Esc cerrar",
        Key::PaletteActionOpenLocalGraph => "Abrir el gráfico local",
        Key::PaletteActionSwitchVault => "Cambiar de bóveda",
        Key::PaletteActionOpenSettings => "Abrir los ajustes",
        Key::PaletteActionNewNote => "Nueva nota",
        Key::PaletteActionTogglePreview => "Alternar escritura / lectura",
        Key::PaletteActionToggleGraph => "Alternar gráfico",
        Key::PaletteActionToggleBoard => "Alternar tablero",
        Key::PaletteActionToggleKanban => "Alternar Kanban",
        Key::PaletteActionToggleFocusMode => "Alternar el modo de concentración",
        Key::PaletteActionNewKanban => "Nuevo Kanban",
        Key::PaletteActionConvertToKanban => "Convertir una nota vacía en Kanban",
        Key::PaletteActionImportDocument => "Importar documento…",
        Key::PaletteHintGraphLocal => "Gráfico · Local",
        Key::PaletteHintVaults => "Bóvedas",
        Key::PaletteHintHelp => "Ayuda",
        Key::PaletteHintKanban => "Kanban",
        Key::PaletteKeywordsOpenLocalGraph => "gráfico local enfoque vecinos alcance",
        Key::PaletteKeywordsSwitchVault => "bóveda abrir reciente carpeta cambiar",
        Key::PaletteKeywordsOpenSettings => "ajustes preferencias opciones configuración ayuda",
        Key::PaletteKeywordsNewNote => "nueva nota crear añadir",
        Key::PaletteKeywordsTogglePreview => "escritura lectura vista previa editor modo",
        Key::PaletteKeywordsToggleGraph => "gráfico global local modo",
        Key::PaletteKeywordsToggleBoard => "tablero pizarra excalidraw dibujo lienzo boceto modo",
        Key::PaletteKeywordsToggleKanban => "kanban columnas tarjetas tareas modo",
        Key::PaletteKeywordsToggleFocusMode => {
            "concentración zen escribir sin distracciones ocultar interfaz atenuar"
        }
        Key::PaletteKeywordsNewKanban => "kanban crear añadir columnas tarjetas tareas",
        Key::PaletteKeywordsConvertToKanban => {
            "kanban vacía en blanco convertir columnas tarjetas tareas"
        }
        Key::PaletteKeywordsImportDocument => {
            "importar documento docx doc word pdf office odt rtf epub csv"
        }
        Key::PaletteSectionRecent => "Recientes",
        Key::PaletteSectionNotes => "Notas",
        Key::PaletteSectionActions => "Acciones",
        Key::PaletteSectionKanbans => "Kanbans",
        Key::BoardPageTitle => "Tableros",
        Key::BoardPageNew => "Nuevo tablero",
        Key::BoardPageEmptyTitle => "Aún no hay tableros",
        Key::BoardPageEmptyDetail => "Crea uno con Nuevo tablero",
        Key::BoardPageEmptyHint => {
            "Un tablero es un archivo Excalidraw en tu bóveda. Crea uno para empezar a dibujar."
        }
        Key::BoardPageNoSelectionTitle => "Ningún tablero seleccionado",
        Key::BoardPageNoSelectionDetail => {
            "Elige un tablero del selector de arriba para ver qué contiene"
        }
        Key::BoardPageTitleLabel => "Nombre del tablero; doble clic para cambiar el nombre",
        Key::BoardPageUnreadableTitle => "No se pudo leer este tablero",
        Key::BoardPageNoOpenTitle => "Ningún tablero abierto",
        Key::BoardPageNoOpenDetail => {
            "Elige un tablero del selector de arriba o crea uno nuevo. Los tableros son archivos Excalidraw en tu bóveda."
        }
        Key::BoardPageEmptyCanvas => "Aún no hay nada dibujado en este tablero",
        Key::BoardPageHint => {
            "Clic/arrastrar seleccionar · Del eliminar · ⌘Z deshacer · R D O A L P dibujar · H desplazar · ⌘L vincular nota · ⌘S guardar · 0 ajustar"
        }
        Key::BoardPagePickerLabel => "Elegir un tablero",
        Key::BoardPagePickerHelp => "Abrir la lista de tableros",
        Key::BoardPagePickerMenu => "Tableros",
        Key::BoardToolToolbar => "Herramientas del tablero",
        Key::BoardToolHand => "Mano",
        Key::BoardToolHandHelp => "Desplazar el tablero (H)",
        Key::BoardToolSelect => "Seleccionar",
        Key::BoardToolSelectHelp => "Seleccionar y mover elementos (V)",
        Key::BoardToolRectangle => "Rectángulo",
        Key::BoardToolRectangleHelp => "Dibujar un rectángulo (R)",
        Key::BoardToolDiamond => "Rombo",
        Key::BoardToolDiamondHelp => "Dibujar un rombo (D)",
        Key::BoardToolEllipse => "Elipse",
        Key::BoardToolEllipseHelp => "Dibujar una elipse (O)",
        Key::BoardToolArrow => "Flecha",
        Key::BoardToolArrowHelp => "Dibujar una flecha (A)",
        Key::BoardToolLine => "Línea",
        Key::BoardToolLineHelp => "Dibujar una línea (L)",
        Key::BoardToolFreedraw => "Trazo libre",
        Key::BoardToolFreedrawHelp => "Dibujar a mano alzada (P o X)",
        Key::BoardToolText => "Texto",
        Key::BoardToolTextHelp => "Colocar texto y luego escribir (T)",
        Key::BoardLinkTitle => "Vincular nota",
        Key::BoardLinkHelp => "Vincular los elementos seleccionados a una nota (⌘L)",
        Key::BoardLinkName => "Nombre o ruta de la nota",
        Key::BoardLinkHint => "un nombre o una ruta de la bóveda · vacío lo quita",
        Key::BoardRenameName => "Nombre del tablero",
        Key::BoardPaletteTitle => "Colores del tablero",
        Key::BoardPaletteHelp => "Colores de trazo y relleno",
        Key::BoardPaletteOpen => "Paleta de colores",
        Key::BoardPaletteClose => "Cerrar la paleta de colores",
        Key::BoardPaletteStroke => "Trazo",
        Key::BoardPaletteFill => "Relleno",
        Key::BoardPaletteSwatchBlack => "negro",
        Key::BoardPaletteSwatchGrey => "gris",
        Key::BoardPaletteSwatchRed => "rojo",
        Key::BoardPaletteSwatchOrange => "naranja",
        Key::BoardPaletteSwatchYellow => "amarillo",
        Key::BoardPaletteSwatchGreen => "verde",
        Key::BoardPaletteSwatchBlue => "azul",
        Key::BoardPaletteSwatchPurple => "morado",
        Key::BoardPaletteSwatchTransparent => "transparente",
        Key::GraphPageEmpty => "No hay notas que dibujar",
        Key::GraphPageFindNode => "Buscar un nodo del gráfico",
        Key::GraphPageFindPlaceholder => "Buscar nodo · Enter",
        Key::GraphPageSelected => "SELECCIONADO",
        Key::GraphPageHint => {
            "Flechas enfocar · Enter abrir · +/- acercar/alejar · 0/F ajustar · I aislar · Arrastrar desplazar · Rueda acercar/alejar"
        }
        Key::GraphScopeGlobal => "Global",
        Key::GraphScopeLocal => "Local",
        Key::GraphActionFit => "Ajustar",
        Key::GraphActionReset => "Restablecer",
        Key::GraphActionIsolate => "Aislar",
        Key::GraphActionOpenNote => "Abrir nota",
        Key::KanbanSectionDatesTitle => "Fechas y horas",
        Key::KanbanSectionDatesCaption => {
            "Cómo se teclean, se escriben y se muestran las fechas y las horas."
        }
        Key::KanbanSectionTagsTitle => "Etiquetas",
        Key::KanbanSectionTagsCaption => {
            "Chips de etiqueta y la lista de colores de etiqueta que se conserva."
        }
        Key::KanbanSectionMetadataTitle => "Metadatos de la nota vinculada",
        Key::KanbanSectionMetadataCaption => {
            "Claves que se dibujan al pie de la tarjeta desde su primera nota vinculada."
        }
        Key::KanbanSectionArchiveTitle => "Archivo",
        Key::KanbanSectionArchiveCaption => "Qué escribe el archivo y cuántas tarjetas conserva.",
        Key::KanbanSectionBoardTitle => "Tablero y tarjetas",
        Key::KanbanSectionBoardCaption => {
            "Ancho de columna, edición de tarjetas y botones de la cabecera."
        }
        Key::KanbanSectionAppTitle => "Aplicación",
        Key::KanbanSectionAppCaption => {
            "Opciones para todos los tableros; no hay anulación por tablero."
        }
        Key::KanbanChoiceEnter => "Enter",
        Key::KanbanChoiceShiftEnter => "Shift+Enter",
        Key::KanbanChoiceStart => "Inicio",
        Key::KanbanChoiceEnd => "Final",
        Key::KanbanChoicePrepend => "Anteponer",
        Key::KanbanChoiceAppend => "Añadir al final",
        Key::KanbanUiDefaultsNote => {
            "El pie de ajustes del propio tablero tiene prioridad; si no, estos valores se aplican a todos los tableros."
        }
        Key::KanbanUiBoardSettings => "Ajustes del tablero",
        Key::KanbanUiChangesWriteOnly => {
            "Los cambios escriben solo esa clave en el pie de ajustes de esta nota y se pueden deshacer."
        }
        Key::KanbanUiNoMetadataKeys => "No hay claves de metadatos",
        Key::KanbanUiNoTagColours => "No hay colores de etiqueta",
        Key::KanbanUiKeptAsWritten => "Se conserva tal cual. Verlo nunca lo reescribe.",
        Key::KanbanUiSourceBoardIgnored => "Valor del tablero ignorado",
        Key::KanbanUiSourceBoardOverride => "Anulación del tablero",
        Key::KanbanUiSourceAppIgnored => "Valor de la aplicación ignorado",
        Key::KanbanUiSourceAppDefault => "Valor predeterminado de la aplicación",
        Key::KanbanUiSourceDefault => "Predeterminado",
        Key::KanbanUiResetBuiltIn => "Restablecer al valor integrado",
        Key::KanbanUiResetBoard => "Restablecer el valor del tablero",
        Key::KanbanUiResetAppDefault => "Restablecer al valor de la aplicación",
        Key::KanbanUiRemove => "Quitar",
        Key::KanbanUiAdd => "Añadir",
        Key::KanbanUiContainsMarkdown => "Contiene Markdown",
        Key::KanbanUiNewMetadataKey => "Nueva clave de metadatos",
        Key::KanbanUiUnlimited => "Sin límite",
        Key::KanbanPageTitle => "Kanban",
        Key::KanbanPageViewSwitcher => "Vista de Kanban",
        Key::KanbanPageFilter => "Filtrar",
        Key::KanbanPageClearFilters => "Limpiar filtros",
        Key::KanbanPageOpenMarkdown => "Abrir como Markdown",
        Key::KanbanPageResolveConflict => "Resolver conflicto",
        Key::KanbanPageEmptyTitle => "Ningún kanban abierto",
        Key::KanbanPageEmptyDetail => {
            if cfg!(target_os = "macos") {
                "Selecciona una nota de kanban y pulsa ⌘5 para verla como tablero."
            } else {
                "Selecciona una nota de kanban y pulsa Ctrl+5 para verla como tablero."
            }
        }
        Key::KanbanPageInvalidTitle => "Esta nota no se puede mostrar como kanban",
        Key::KanbanPageInvalidDetail => {
            "Su pie de ajustes está incompleto o no es JSON válido. El archivo se deja tal cual."
        }
        Key::KanbanLaneComplete => "Completada",
        Key::KanbanLaneNew => "Nueva columna",
        Key::KanbanLaneAdd => "Añadir columna",
        Key::KanbanLaneTitle => "Título de la columna",
        Key::KanbanLaneCardLimit => "Límite de tarjetas",
        Key::KanbanLaneNoLimit => "Sin límite",
        Key::KanbanCardText => "Texto de la tarjeta",
        Key::KanbanCardOpenLink => "Abrir enlace",
        Key::KanbanViewBoard => "Tablero",
        Key::KanbanViewList => "Lista",
        Key::KanbanViewTable => "Tabla",
        Key::KanbanViewBoardTab => "Vista de tablero",
        Key::KanbanViewListTab => "Vista de lista",
        Key::KanbanViewTableTab => "Vista de tabla",
        Key::KanbanViewNoLanes => "No hay columnas en este tablero",
        Key::KanbanViewNoCards => "No hay tarjetas",
        Key::KanbanViewNoMatchingCards => "No hay tarjetas coincidentes",
        Key::KanbanViewCardsByLane => "Tarjetas por columna",
        Key::KanbanViewColumns => "Columna, Tarjeta, Fecha, Etiquetas, Progreso",
        Key::KanbanViewColumnLane => "Columna",
        Key::KanbanViewColumnCard => "Tarjeta",
        Key::KanbanViewColumnDate => "Fecha",
        Key::KanbanViewColumnTags => "Etiquetas",
        Key::KanbanViewColumnProgress => "Progreso",
        Key::KanbanFilterFind => "Buscar en el tablero",
        Key::KanbanFilterTitle => "Filtrar tarjetas",
        Key::KanbanFilterText => "Texto",
        Key::KanbanFilterDate => "Fecha",
        Key::KanbanFilterAnyDate => "Cualquier fecha",
        Key::KanbanFilterOverdue => "Vencidas",
        Key::KanbanFilterStatus => "Estado",
        Key::KanbanFilterAny => "Cualquiera",
        Key::KanbanFilterOpen => "Abierta",
        Key::KanbanFilterDone => "Completada",
        Key::KanbanFilterClearAll => "Limpiar todo",
        Key::KanbanFilterDateError => "Un filtro de fecha usa YYYY-MM-DD; el rango no se aplica.",
        Key::KanbanFilterPreviousMatch => "Coincidencia anterior",
        Key::KanbanFilterNextMatch => "Coincidencia siguiente",
        Key::KanbanFilterCloseFind => "Cerrar la búsqueda",
        Key::KanbanArchiveEmpty => "No hay tarjetas archivadas.",
        Key::KanbanArchiveDelete => "Eliminar",
        Key::KanbanArchiveRestore => "Restaurar",
        Key::KanbanArchiveRestoreTo => "Restaurar en…",
        Key::KanbanPickerAddDate => "Añadir fecha",
        Key::KanbanPickerAddTime => "Añadir hora",
        Key::KanbanPickerAddADate => "Añadir una fecha",
        Key::KanbanPickerAddATime => "Añadir una hora",
        Key::KanbanPickerHour => "Hora",
        Key::KanbanPickerMinute => "Minuto",
        Key::KanbanPickerNow => "Ahora",
        Key::KanbanPickerPreviousMonth => "Mes anterior",
        Key::KanbanPickerNextMonth => "Mes siguiente",
        Key::KanbanPickerDays => "Días",
        Key::KanbanSettingDateTriggerLabel => "Disparador de fecha",
        Key::KanbanSettingDateTriggerHelp => {
            "Se escribe antes de una fecha, como en @{2026-11-12}."
        }
        Key::KanbanSettingTimeTriggerLabel => "Disparador de hora",
        Key::KanbanSettingTimeTriggerHelp => "Se escribe antes de una hora, como en @@{14:30}.",
        Key::KanbanSettingDateFormatLabel => "Formato de fecha",
        Key::KanbanSettingDateFormatHelp => {
            "Se escribe dentro de un token de fecha nuevo; YYYY, MM, DD, MMMM y Do son tokens."
        }
        Key::KanbanSettingTimeFormatLabel => "Formato de hora",
        Key::KanbanSettingTimeFormatHelp => {
            "Se escribe dentro de un token de hora nuevo; HH, mm, h y A son tokens."
        }
        Key::KanbanSettingDateDisplayFormatLabel => "Formato de visualización de fecha",
        Key::KanbanSettingDateDisplayFormatHelp => {
            "Cómo se muestra una fecha al pie de la tarjeta."
        }
        Key::KanbanSettingShowRelativeDateLabel => "Mostrar fecha relativa",
        Key::KanbanSettingShowRelativeDateHelp => "Añade hoy o dentro de N días junto a la fecha.",
        Key::KanbanSettingHideDateInTitleLabel => "Ocultar la fecha en el título",
        Key::KanbanSettingHideDateInTitleHelp => {
            "Mantiene los tokens de fecha y hora fuera del texto principal de la tarjeta."
        }
        Key::KanbanSettingLinkDateToDailyNoteLabel => "Vincular la fecha a la nota diaria",
        Key::KanbanSettingLinkDateToDailyNoteHelp => {
            "Hace que la fecha mostrada sea un enlace a su nota diaria."
        }
        Key::KanbanSettingHideTagsInTitleLabel => "Ocultar las etiquetas en el título",
        Key::KanbanSettingHideTagsInTitleHelp => {
            "Mantiene las etiquetas (#) fuera del texto principal de la tarjeta."
        }
        Key::KanbanSettingShowTagsAtBottomLabel => "Mostrar las etiquetas al pie",
        Key::KanbanSettingShowTagsAtBottomHelp => "Dibuja chips de etiqueta al pie de la tarjeta.",
        Key::KanbanSettingTagColorsLabel => "Colores de etiqueta",
        Key::KanbanSettingTagColorsHelp => {
            "Se conserva tal cual. Muninn aún no tiene editor de colores."
        }
        Key::KanbanSettingLinkedPageMetadataLabel => "Metadatos de la nota vinculada",
        Key::KanbanSettingLinkedPageMetadataHelp => {
            "Claves que se muestran al pie de la tarjeta desde su primera nota vinculada."
        }
        Key::KanbanSettingShowMissingMetadataLabel => "Mostrar metadatos ausentes",
        Key::KanbanSettingShowMissingMetadataHelp => {
            "Dibuja clave: — cuando faltan la nota vinculada o la clave."
        }
        Key::KanbanSettingArchiveWithDateLabel => "Archivar con fecha",
        Key::KanbanSettingArchiveWithDateHelp => {
            "Añade la fecha al texto de la tarjeta cuando se archiva."
        }
        Key::KanbanSettingArchiveDateFormatLabel => "Formato de fecha del archivado",
        Key::KanbanSettingArchiveDateFormatHelp => {
            "El formato de fecha usado al archivar; los mismos tokens que Formato de fecha."
        }
        Key::KanbanSettingArchiveDatePositionLabel => "Posición de la fecha del archivo",
        Key::KanbanSettingArchiveDatePositionHelp => {
            "Inicio o final del texto de la tarjeta archivada."
        }
        Key::KanbanSettingArchiveDateSeparatorLabel => "Separador de la fecha del archivo",
        Key::KanbanSettingArchiveDateSeparatorHelp => {
            "Texto entre la tarjeta y su fecha; un valor vacío las une."
        }
        Key::KanbanSettingMaxArchiveCardsLabel => "Máximo de tarjetas en el archivo",
        Key::KanbanSettingMaxArchiveCardsHelp => {
            "Las tarjetas archivadas más antiguas por encima de este número se descartan. 0 significa sin límite."
        }
        Key::KanbanSettingLaneWidthLabel => "Ancho de columna",
        Key::KanbanSettingLaneWidthHelp => {
            "Ancho de columna en puntos; el tablero limita lo que dibuja a 120–720."
        }
        Key::KanbanSettingNewLineTriggerLabel => "Disparador de nueva línea",
        Key::KanbanSettingNewLineTriggerHelp => {
            "En el editor de tarjetas, Enter confirma y Shift+Enter hace un salto de línea, o al revés."
        }
        Key::KanbanSettingNewCardInsertionLabel => "Posición de las tarjetas nuevas",
        Key::KanbanSettingNewCardInsertionHelp => {
            "Dónde coloca la tarjeta nueva el botón + Añadir tarjeta."
        }
        Key::KanbanSettingShowCheckboxesLabel => "Mostrar casillas",
        Key::KanbanSettingShowCheckboxesHelp => "Dibuja la casilla de completado de cada tarjeta.",
        Key::KanbanSettingShowFilterButtonLabel => "Botón de filtro",
        Key::KanbanSettingShowFilterButtonHelp => "Muestra el botón Filtrar de la cabecera.",
        Key::KanbanSettingShowArchiveButtonLabel => "Botón Archivar",
        Key::KanbanSettingShowArchiveButtonHelp => "Muestra el botón Archivar de la cabecera.",
        Key::KanbanSettingShowOpenMarkdownButtonLabel => "Botón Abrir como Markdown",
        Key::KanbanSettingShowOpenMarkdownButtonHelp => {
            "Muestra el botón Abrir como Markdown de la cabecera."
        }
        Key::KanbanSettingNewNoteFolderLabel => "Carpeta de notas nuevas",
        Key::KanbanSettingNewNoteFolderHelp => {
            "Carpeta para Nueva nota desde tarjeta; si está vacía, se usa la carpeta de Nueva nota."
        }
        Key::KanbanSettingNewNoteTemplateLabel => "Plantilla de nota nueva",
        Key::KanbanSettingNewNoteTemplateHelp => {
            "Una ruta de nota cuyo texto inicia la nota nueva; si está vacía, se usa la plantilla de nota de la aplicación."
        }
        Key::KanbanSettingKanbanAutosaveLabel => "Guardado automático de los cambios del kanban",
        Key::KanbanSettingKanbanAutosaveHelp => {
            "Guarda los cambios del kanban pendientes cuando dejas de editar. Desactivado deja el guardado a ⌘S."
        }
        Key::StatusSaving => "Guardando…",
        Key::StatusSaved => "Guardado",
        Key::StatusAutosaveOn => "Guardado automático activo",
        Key::StatusRetry => "Reintentar",
        Key::StatusUndo => "Deshacer",
        Key::RecoveryRestore => "Restaurar",
        Key::RecoveryRestoreAll => "Restaurar todo",
        Key::RecoveryDismiss => "Descartar",
        Key::RecoveryWriteEdits => "— Cambios de escritura",
        Key::RecoveryKanbanEdits => "— Cambios de Kanban",
        Key::BrokenAllResolve => "Todos los enlaces se resuelven",
        Key::BrokenTitle => "Enlaces rotos",
        Key::TopbarNoNoteSelected => "Ninguna nota seleccionada",
        Key::TopbarOpenAsKanban => "Abrir como kanban",
        Key::TrashTitle => "Papelera",
        Key::TrashEmptyAction => "Vaciar la papelera",
        Key::TrashExplainer => {
            "Las notas eliminadas permanecen aquí hasta deshacer o eliminarlas de forma permanente. Se conservan como máximo 500 archivos o 100 MB; primero se poda la carpeta con la marca de tiempo más antigua."
        }
        Key::TrayShow => "Mostrar Muninn",
        Key::TrayNewNote => "Nueva nota",
        Key::TraySyncNow => "Sincronizar ahora",
        Key::TrayQuit => "Salir de Muninn",
        Key::TrayNotConnected => "Sin conexión.",
        Key::TrayTooltip => "Muninn",
        Key::ErrorEnterVaultPath => "Introduce una ruta de carpeta de bóveda",
        Key::ErrorEnterNewVaultPath => "Introduce una ruta de carpeta para la bóveda nueva",
        Key::ErrorSaveNoteBeforeSwitch => {
            "Guarda o resuelve la nota actual antes de cambiar de bóveda"
        }
        Key::ErrorSaveBoardBeforeSwitch => "Guarda el tablero actual antes de cambiar de bóveda",
        Key::ErrorSaveKanbanBeforeSwitch => "Guarda el kanban actual antes de cambiar de bóveda",
        Key::ErrorSaveNoteBeforeRestore => {
            "Guarda o descarta la nota actual antes de restaurar otra"
        }
        Key::ErrorSaveBoardBeforeRestore => {
            "Guarda o descarta el tablero actual antes de restaurar otro"
        }
        Key::ErrorSaveBoardBeforeRescan => {
            "Guarda el tablero actual antes de volver a escanear la bóveda"
        }
        Key::ErrorOpenerHintUnix => {
            "Instala xdg-utils (para xdg-open) o abre el archivo manualmente."
        }
        Key::ErrorNoFileOpener => {
            "No hay ninguna aplicación documentada para abrir archivos en esta plataforma; abre el archivo manualmente"
        }
        Key::ErrorOpenerHintMacos => {
            "El comando `open` del sistema falló; abre el archivo desde Finder."
        }
        Key::ErrorOpenerHintWindows => {
            "La aplicación del sistema no pudo abrir el archivo; ábrelo desde el Explorador."
        }
        Key::ErrorOpenerHintOther => "Abre el archivo manualmente.",
        Key::ErrorDocumentBoard => "tablero",
        Key::ErrorDocumentKanban => "kanban",
        Key::ImportDocument => "Importar documento…",
        Key::ImportDocumentHelp => "Importar un documento como nota nueva",
        Key::ImportDialogTitle => "Importar documento",
        Key::ImportDialogFilterDocuments => "Documentos",
        Key::ErrorImportDialogUnavailable => {
            "No hay diálogo de archivos en este escritorio; arrastra el documento a la ventana."
        }
        Key::StatusTrashEmptied => "Papelera vaciada",
        Key::StatusLinkCopied => "Enlace a la tarjeta copiado",
        Key::ConflictExternalChange => "Cambio externo detectado",
        Key::ConflictFileDeleted => "archivo eliminado externamente",
        Key::ConflictChangedOnDisk => "Cambiado en disco por otra aplicación",
        Key::ConflictMissingOnDisk => "Eliminado o renombrado en disco",
        Key::ConflictKeepMine => "Mantener la mía",
        Key::ConflictLoadTheirs => "Cargar la suya",
        Key::ConflictSaveCopy => "Guardar copia",
        Key::ConflictDiscard => "Descartar",
        Key::CommonAppName => "Muninn",
        Key::CommonDelete => "Eliminar",
        Key::CommonEdit => "Editar",
        Key::CommonMove => "Mover",
        Key::CommonNewNote => "Nueva nota",
        Key::CommonOpen => "Abrir",
        Key::CommonRename => "Renombrar",
        Key::CommonResults => "Resultados",
        Key::CommonVaultRoot => "Raíz de la bóveda",
        Key::CommonDismiss => "Descartar",
        Key::CommonOn => "Activado",
        Key::CommonOff => "Desactivado",
        Key::SidebarNoNotesTitle => "Aún no hay notas",
        Key::SidebarNoNotesHint => "Añade archivos .md aquí o crea uno abajo.",
        Key::SidebarNoMatchTitle => "Ninguna nota coincide",
        Key::SidebarNoMatchHint => "Prueba con otra búsqueda.",
        Key::SidebarSearch => "Buscar notas",
        Key::SidebarLinksOk => "enlaces correctos",
        Key::SidebarKanbans => "KANBANS",
        Key::SidebarBoards => "TABLEROS",
        Key::SidebarTheVaultRoot => "la raíz de la bóveda",
        Key::SettingsTitle => "Ajustes",
        Key::TopbarSettingsHelp => "Ajustes (Ctrl+,)",
        Key::TopbarHideSidebar => "Ocultar la barra lateral",
        Key::TopbarShowSidebar => "Mostrar la barra lateral",
        Key::SettingsChangesSave => "Los cambios se guardan automáticamente.",
        Key::SettingsClose => "Cerrar los ajustes (Esc)",
        Key::SettingsSection => "Sección de ajustes",
        Key::SettingsAutosave => "Guardado automático",
        Key::SettingsDiagnostics => "Diagnóstico",
        Key::SettingsCopyDiagnostics => "Copiar el diagnóstico",
        Key::SettingsReportIssue => "Informar de un problema en GitHub",
        Key::SettingsThemeOmarchy => "Omarchy",
        Key::SettingsThemeSystem => "Sistema",
        Key::SettingsThemeDark => "Oscuro",
        Key::SettingsThemeLight => "Claro",
        Key::SettingsMotionSystem => "Sistema",
        Key::SettingsMotionOn => "Activado",
        Key::SettingsMotionOff => "Desactivado",
        Key::SettingsEffectsClassic => "Clásico",
        Key::SettingsEffectsElectric => "Eléctrico",
        Key::SettingsEffectsOff => "Desactivado / Mínimo",
        Key::SettingsIntensitySubtle => "Sutil",
        Key::SettingsIntensityBalanced => "Equilibrado",
        Key::SettingsIntensityIntense => "Intenso",
        Key::WelcomeGraphLabel => "Abrir el gráfico de enlaces",
        Key::WelcomeGraphHelp => "Mira cómo se conectan las notas mediante [[enlaces]]",
        Key::WelcomeGraphPill => "[[una nota]] → gráfico",
        Key::PreviewEmptyNote => "Esta nota está vacía. Escribe algo en el editor.",
        Key::PreviewStartWriting => "Crea una nota para empezar a escribir.",
        Key::PreviewEditor => "Editor de notas",
        Key::PreviewEditorPlaceholder => "Empieza a escribir. Escribe [[ para enlazar otra nota.",
        Key::PreviewCompletions => "Autocompletado",
        Key::PreviewAttachments => "ADJUNTOS",
        Key::PreviewAttachmentsNote => "Los archivos quedan fuera del índice de notas",
        Key::PreviewOpen => "Abrir",
        Key::PreviewOpenBoard => "Abrir tablero",
        Key::PreviewMissingFile => "archivo ausente",
        Key::PreviewBoardKind => "tablero",
        Key::PreviewAttachmentKind => "adjunto",
        Key::PreviewMissing => "ausente",
        Key::PreviewNoOutgoing => "No hay enlaces salientes",
        Key::PreviewNoBacklinks => "Aún no hay enlaces entrantes",
        Key::PreviewLinks => "ENLACES",
        Key::PreviewOutgoing => "SALIENTES",
        Key::PreviewBacklinks => "ENTRANTES",
        Key::PreviewGraph => "GRÁFICO",
        Key::PreviewGraphHint => "Mira esta nota y todo lo que toca.",
        Key::PreviewFind => "Buscar en la nota",
        Key::StartupScanning => "Escaneando tu bóveda",
        Key::StartupScanCancelled => "Escaneo cancelado",
        Key::StartupCouldNotOpen => "No se pudo abrir la bóveda",
        Key::StartupRetryHint => {
            "Reintentar vuelve a escanear la bóveda o elige otra carpeta en Ajustes (⌘,)."
        }
        Key::StartupPickFolderHint => {
            "Elige otra carpeta en Ajustes (⌘,), o pasa --vault / define MUNINN_VAULT."
        }
        Key::ModeWrite => "Escritura",
        Key::ModeRead => "Lectura",
        Key::ModeGraph => "Gráfico",
        Key::ModeBoard => "Tablero",
        Key::ModeKanban => "Kanban",
        Key::FocusExit => "Salir del modo de concentración",
        Key::FocusEnter => "Modo de concentración",
        Key::MenuOpenExternally => "Abrir externamente",
        Key::MenuReveal => "Mostrar en el gestor de archivos",
        Key::MenuMoveTo => "Mover a…",
        Key::MenuNewNoteHere => "Nueva nota aquí",
        Key::MenuNewFolder => "Nueva carpeta…",
        Key::MenuInsertBefore => "Insertar tarjeta antes",
        Key::MenuInsertAfter => "Insertar tarjeta después",
        Key::MenuDuplicate => "Duplicar",
        Key::MenuMarkOpen => "Marcar como abierta",
        Key::MenuMarkDone => "Marcar como completada",
        Key::MenuChangeDate => "Cambiar fecha…",
        Key::MenuAddDate => "Añadir una fecha…",
        Key::MenuChangeTime => "Cambiar hora…",
        Key::MenuAddTime => "Añadir una hora…",
        Key::MenuSplitCard => "Dividir tarjeta",
        Key::MenuCopyLink => "Copiar enlace a la tarjeta",
        Key::MenuNewNoteFromCard => "Nueva nota desde la tarjeta",
        Key::MenuMarkAllDone => "Marcar todas las tarjetas como completadas",
        Key::MenuNoteActions => "Acciones de la nota",
        Key::MenuFolderActions => "Acciones de la carpeta",
        Key::MenuBoardActions => "Acciones del tablero",
        Key::MenuCardActions => "Acciones de la tarjeta",
        Key::MenuLaneActions => "Acciones de la columna",
        Key::MenuArchiveActions => "Acciones del archivo",
        Key::MenuMoveToLane => "Mover a la columna",
        Key::LinkImpactHere => "aquí",
        Key::LinkImpactIntoFolder => "para esta carpeta",
        Key::LinkThisNote => "Esta nota",
        Key::LinkNameFallback => "Enlace",
        Key::RenameRewriteLinks => "Reescribir los enlaces al nombre nuevo",
        Key::RenameLeaveDangling => "Dejar los enlaces sin resolver",
        Key::DeleteNoteQuestion => "¿Eliminar esta nota?",
        Key::DeleteFolderQuestion => "¿Eliminar la carpeta? (solo si está vacía)",
        Key::NewNoteName => "Nombre de la nota nueva",
        Key::NoteNamePlaceholder => "Nombre de la nota",
        Key::RenameNoteName => "Renombrar nota",
        Key::FolderNamePlaceholder => "Nombre de la carpeta",
        Key::NewFolderName => "Nombre de la carpeta nueva",
        Key::RenameFolderName => "Renombrar carpeta",
        Key::PaletteMissingNote => "Nota ausente",
        Key::PaletteKanbanRoot => "Kanban · Raíz de la bóveda",
        Key::BoardRenameSaveFirst => "Guarda primero el tablero: no se pudo guardar",
        Key::KanbanFilterToday => "hoy",
        Key::KanbanFilterOverdueWord => "vencida",
        Key::KanbanFilterOpenWord => "abierta",
        Key::KanbanFilterDoneWord => "completada",
        Key::KanbanDailyNoteLink => "Abrir la nota diaria de la fecha",
        Key::KanbanLaneMoveUp => "Subir",
        Key::KanbanLaneMoveTop => "Mover al principio",
        Key::KanbanLaneMoveDown => "Bajar",
        Key::KanbanLaneMoveBottom => "Mover al final",
        Key::KanbanLaneMovePreviousList => "Mover a la lista anterior",
        Key::KanbanLaneMoveNextList => "Mover a la lista siguiente",
        Key::KanbanLaneMoveToList => "Mover a la lista…",
        Key::KanbanLaneInsertBefore => "Insertar columna antes",
        Key::KanbanLaneInsertAfter => "Insertar columna después",
        Key::KanbanLaneMoveLeft => "Mover la columna a la izquierda",
        Key::KanbanLaneMoveRight => "Mover la columna a la derecha",
        Key::KanbanLaneRename => "Renombrar columna",
        Key::KanbanLaneLimit => "Límite de tarjetas…",
        Key::KanbanLaneMarkNotDone => "Marcar como columna no completada",
        Key::KanbanLaneMarkDone => "Marcar como columna completada",
        Key::KanbanLaneSortText => "Ordenar por texto",
        Key::KanbanLaneSortDate => "Ordenar por fecha",
        Key::KanbanLaneSortTags => "Ordenar por etiquetas",
        Key::KanbanLaneDelete => "Eliminar columna",
        Key::KanbanLaneDeleteCards => "Eliminar la columna y sus tarjetas",
        Key::KanbanLaneDeleteText => "Eliminar la columna y su texto de nota",
        Key::KanbanCardFallback => "Tarjeta",
        Key::KanbanCardMarkDone => "Marcar como completada",
        Key::KanbanCardMarkOpen => "Marcar como no completada",
        Key::KanbanCardChecklistItem => "Elemento de la lista de tareas",
        Key::KanbanEditAddCard => "añadir la tarjeta",
        Key::KanbanEditEditCard => "editar la tarjeta",
        Key::KanbanEditDeleteCard => "eliminar la tarjeta",
        Key::KanbanEditDuplicateCard => "duplicar la tarjeta",
        Key::KanbanEditSplitCard => "dividir la tarjeta",
        Key::KanbanEditChangeDoneMark => "cambiar la marca de completado",
        Key::KanbanEditToggleChecklistItem => "alternar el elemento de la lista de tareas",
        Key::KanbanEditMarkCardsDone => "marcar las tarjetas como completadas",
        Key::KanbanEditAddLane => "añadir la columna",
        Key::KanbanEditRenameLane => "renombrar la columna",
        Key::KanbanEditClearLimit => "borrar el límite",
        Key::KanbanEditSetLimit => "establecer el límite",
        Key::KanbanEditDeleteLane => "eliminar la columna",
        Key::KanbanEditChangeDoneLane => "cambiar la columna de completadas",
        Key::KanbanEditSortLane => "ordenar la columna",
        Key::KanbanEditCollapseLane => "contraer la columna",
        Key::KanbanEditMoveCard => "mover la tarjeta",
        Key::KanbanEditChangeDate => "cambiar la fecha",
        Key::KanbanEditChangeTime => "cambiar la hora",
        Key::KanbanEditClearDate => "borrar la fecha",
        Key::KanbanEditClearTime => "borrar la hora",
        Key::KanbanEditChangeBoardSetting => "cambiar el ajuste del tablero",
        Key::KanbanEditResetBoardSetting => "restablecer el ajuste del tablero",
        Key::KanbanEditCopyCardLink => "copiar el enlace de la tarjeta",
        Key::KanbanEditMakeNoteFromCard => "crear una nota a partir de la tarjeta",
        Key::KanbanEditArchiveCard => "archivar la tarjeta",
        Key::KanbanEditRestoreCard => "restaurar la tarjeta archivada",
        Key::KanbanEditDeleteArchivedCard => "eliminar la tarjeta archivada",
        Key::KanbanStatusLaneRefused => "la columna que se estaba editando fue rechazada",
        Key::KanbanStatusCardRefused => "la tarjeta que se estaba editando fue rechazada",
        Key::KanbanStatusNotInsideVault => "el kanban no está dentro de la bóveda abierta",
        Key::KanbanStatusNoteGone => "la nota ya no está en la bóveda",
        Key::KanbanStatusFileUnchecked => "no se pudo comprobar el archivo",
        Key::KanbanStatusFileRemoved => "el archivo se eliminó",
        Key::KanbanStatusFileChanged => "el archivo cambió en disco",
        Key::KanbanTimeInvalid => "La hora debe estar entre 0 y 23, y los minutos entre 0 y 59",
        Key::KanbanLaneLimitInvalid => "El límite de tarjetas debe ser un número entero",
        Key::VaultEmptyFolderName => "nombre de carpeta vacío",
        Key::VaultCannotRenameRoot => "no se puede renombrar la raíz",
        Key::VaultCannotDeleteRoot => "no se puede eliminar la raíz",
        Key::VaultDestinationExists => "el destino ya existe",
        Key::BoardParseNotObject => "un tablero debe ser un objeto JSON",
        Key::BoardParseNotExcalidraw => {
            "no es una escena de Excalidraw (el tipo no es «excalidraw»)"
        }
        Key::BoardParseVersionNumber => "la versión de la escena debe ser un número entero",
        Key::BoardParseSourceString => "la fuente de la escena debe ser una cadena",
        Key::BoardParseElementsArray => "los elementos de la escena deben ser una matriz",
        Key::BoardStatusUnreadable => "no se pudo leer el tablero, así que no se escribe",
        Key::BoardStatusOtherVault => "el tablero pertenece a otra bóveda",
        Key::BoardStatusInsideVault => "el tablero no está dentro de la bóveda abierta",
        Key::KanbanEditMoveLane => "mover la columna",
        Key::AutostartPathNotUtf8 => "la ruta del ejecutable no es UTF-8",
        Key::AutostartPathLineBreak => "la ruta del ejecutable contiene un salto de línea",
        Key::NoteNoLongerIndexed => "la nota ya no está indexada",
        Key::TrashEmpty => "La papelera está vacía",
        Key::CoreBoardNameEmpty => "no puede estar vacío",
        Key::CoreBoardNameSeparators => "no puede contener / ni \\",
        Key::CoreBoardNameHidden => "no puede empezar por un punto",
        Key::CoreBoardNameForbidden => "no puede contener : * ? \" < > | ni caracteres de control",
        Key::CoreBoardNameTrailingDot => "no puede terminar en punto",
        Key::CoreBoardNameReserved => "no puede ser un nombre de dispositivo reservado",
    }
}
