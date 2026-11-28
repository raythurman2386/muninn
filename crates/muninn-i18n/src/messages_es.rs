//! The `es-ES` template table for [`MsgKey`].
//!
//! Mirrors [`crate::messages`] arm for arm and in the same order. Every
//! placeholder keeps its position in a natural Spanish sentence; the
//! one/other pairs are separate rows, exactly as in English, so a count of 0
//! picks the plural row (`0 notas`) and 1 picks the singular (`1 nota`).
//!
//! The copy is reviewed by a native or fluent speaker before release.

use crate::msg::MsgKey;

/// The Spanish template for `key`.
///
/// Private: callers go through [`crate::phrase`] or [`crate::phrase_current`],
/// which substitute the arguments and pick the language. Keeping every table
/// behind those functions is what makes a second language a change to this
/// crate rather than to call sites.
pub(crate) fn template(key: MsgKey) -> &'static str {
    match key {
        MsgKey::FilesSkippedOne => "{0} archivo omitido",
        MsgKey::FilesSkippedOther => "{0} archivos omitidos",
        MsgKey::NoteOne => "{0} nota",
        MsgKey::NoteOther => "{0} notas",
        MsgKey::BoardOne => "{0} tablero",
        MsgKey::BoardOther => "{0} tableros",
        MsgKey::TargetOne => "{0} destino",
        MsgKey::TargetOther => "{0} destinos",
        MsgKey::ReferrerOne => "{0} referente",
        MsgKey::ReferrerOther => "{0} referentes",
        MsgKey::WordOne => "{0} palabra",
        MsgKey::WordOther => "{0} palabras",
        MsgKey::LinkOne => "{0} enlace",
        MsgKey::LinkOther => "{0} enlaces",
        MsgKey::BacklinkOne => "{0} enlace entrante",
        MsgKey::BacklinkOther => "{0} enlaces entrantes",
        MsgKey::GraphOutgoingLinkOne => "{0} enlace saliente",
        MsgKey::GraphOutgoingLinkOther => "{0} enlaces salientes",
        MsgKey::ResultOne => "{0} resultado",
        MsgKey::ResultOther => "{0} resultados",
        MsgKey::ElementOne => "{0} elemento",
        MsgKey::ElementOther => "{0} elementos",
        MsgKey::ArchivedCardOne => "{0} tarjeta archivada",
        MsgKey::ArchivedCardOther => "{0} tarjetas archivadas",
        MsgKey::LineOne => "{0} línea",
        MsgKey::LineOther => "{0} líneas",
        MsgKey::FileOne => "{0} archivo",
        MsgKey::FileOther => "{0} archivos",
        MsgKey::StatusCounts => "{0} · {1} · {2}",
        MsgKey::TrashSummary => "Papelera · {0} · {1}",
        MsgKey::NoteLinks => "{0} · {1}",
        MsgKey::NoLinks => "sin enlaces",
        MsgKey::NoteLinksRoot => "raíz de la bóveda · {0}",
        MsgKey::NoteLinksFolder => "{0} · {1}",
        MsgKey::SidebarSummary => "{0} · {1}",
        MsgKey::GraphLinks => "{0} · {1}",
        MsgKey::RecoveredCopies => "{0} y {1}",
        MsgKey::MoreEdits => "+{0} más",
        MsgKey::Deleted => "Se eliminó {0}",
        MsgKey::ScanMs => "escaneo {0} ms",
        MsgKey::RestoreTo => "Restaurar en {0}",
        MsgKey::FilterByTag => "Filtrar por #{0}",
        MsgKey::NoteTagFilterSummary => "Notas etiquetadas #{0}",
        MsgKey::LinkImpactOne => "{0} nota enlaza {1}",
        MsgKey::LinkImpactOther => "{0} notas enlazan {1}",
        MsgKey::CouldNotSave => "No se pudo guardar: {0}",
        MsgKey::CouldNotOpenVault => "No se pudo abrir la bóveda: {0}",
        MsgKey::CouldNotReadVault => "No se pudo leer la bóveda: {0}",
        MsgKey::CouldNotReadNoteState => "No se pudo leer el estado de la nota: {0}",
        MsgKey::CouldNotCreateVault => "No se pudo crear la bóveda: {0}",
        MsgKey::CouldNotPersistKanbanRecovery => {
            "No se pudo guardar la copia de recuperación del kanban: {0}"
        }
        MsgKey::CouldNotPersistRecovery => "No se pudo guardar la copia de recuperación: {0}",
        MsgKey::CouldNotPersistBoardRecovery => {
            "No se pudo guardar la copia de recuperación del tablero: {0}"
        }
        MsgKey::CouldNotReadRecovery => "No se pudo leer la copia de recuperación: {0}",
        MsgKey::CouldNotRecreateNoteFolder => "No se pudo volver a crear la carpeta de notas: {0}",
        MsgKey::CouldNotRecreateNote => "No se pudo volver a crear la nota: {0}",
        MsgKey::CouldNotReloadNote => "No se pudo volver a cargar la nota: {0}",
        MsgKey::CouldNotSaveCopy => "No se pudo guardar una copia: {0}",
        MsgKey::CouldNotRenameNote => "No se pudo renombrar la nota: {0}",
        MsgKey::CouldNotRenameFolder => "No se pudo renombrar la carpeta: {0}",
        MsgKey::CouldNotDeleteNote => "No se pudo eliminar la nota: {0}",
        MsgKey::CouldNotDeleteFolder => "No se pudo eliminar la carpeta: {0}",
        MsgKey::CouldNotEmptyTrash => "No se pudo vaciar la papelera: {0}",
        MsgKey::CouldNotMoveNote => "No se pudo mover la nota: {0}",
        MsgKey::CouldNotCreateFolder => "No se pudo crear la carpeta: {0}",
        MsgKey::CouldNotCreateNote => "No se pudo crear una nota: {0}",
        MsgKey::CouldNotAddNoteTemplate => "No se pudo añadir la plantilla de nota: {0}",
        MsgKey::CouldNotBeRead => "no se pudo leer el {0}",
        MsgKey::CouldNotRestore => "No se pudo restaurar {0}: {1}",
        MsgKey::CouldNotOpenFile => "No se pudo abrir {0}: {1}. {2}",
        MsgKey::CouldNotRevealFile => "No se pudo mostrar {0}: {1}. {2}",
        MsgKey::CouldNotImportInto => "No se pudo importar a {0}: {1}",
        MsgKey::RefusingUnsafeRecoveryPath => "Se rechaza la ruta de recuperación no segura {0}",
        MsgKey::BoardQuitRefused => {
            "El tablero no se guardó y no se pudo escribir ninguna copia de recuperación: {0}. Usa Guardar copia o corrige la carpeta y vuelve a salir."
        }
        MsgKey::ScanCancelled => "Escaneo cancelado tras {0} de {1} notas",
        MsgKey::ImportRefused => "Importación rechazada: {0}",
        MsgKey::ImportPartlyRefused => "Importación parcial: {0} de {1}; rechazados: {2}",
        MsgKey::CannotImport => {
            "No se puede importar {0}: arrastra Markdown, imágenes, PDF o documentos de oficina"
        }
        MsgKey::ImportProgress => "Importando {0}…",
        MsgKey::ImportDocumentNotice => "Se importó {0}",
        MsgKey::ImportUnsupported => {
            "No se pudo importar {0}: este tipo de archivo no se puede convertir."
        }
        MsgKey::ImportNeedsOcrOne => {
            "No se pudo importar {0}: la página {1} de {2} es una imagen escaneada y necesita OCR, que Muninn no hace. Exporta primero una versión de texto."
        }
        MsgKey::ImportNeedsOcrOther => {
            "No se pudo importar {0}: las páginas {1} de {2} son imágenes escaneadas y necesitan OCR, que Muninn no hace. Exporta primero una versión de texto."
        }
        MsgKey::ImportEncrypted => {
            "No se pudo importar {0}: el documento está cifrado. Quita su contraseña y vuelve a importarlo."
        }
        MsgKey::ImportMalformed => {
            "No se pudo importar {0}: el archivo está dañado o no es un documento legible."
        }
        MsgKey::ImportResourceLimit => {
            "No se pudo importar {0}: el documento es demasiado complejo para convertirlo."
        }
        MsgKey::ImportMissingPart => {
            "No se pudo importar {0}: al documento le falta una parte que necesita."
        }
        MsgKey::ImportIo => "No se pudo importar {0}: no se pudo leer el archivo.",
        MsgKey::ImportFailed => "No se pudo importar {0}: no se pudo convertir el documento.",
        MsgKey::ImportTooLarge => {
            "No se pudo importar {0}: el archivo ocupa {1}, por encima del límite de {2}."
        }
        MsgKey::ImportBodyTooLarge => {
            "No se pudo importar {0}: la nota convertida ocupa {1}, por encima del límite de {2}."
        }
        MsgKey::RefusedNotAFile => "{0}: no es un archivo",
        MsgKey::RefusedUnsupportedType => "{0}: tipo de archivo no admitido",
        MsgKey::RefusedAlreadyExists => "{0}: ya existe en esta carpeta",
        MsgKey::ConnectedAs => {
            "Conectado como {0}. El token está en el almacén de credenciales del sistema operativo. Pulsa Sincronizar ahora cuando quieras."
        }
        MsgKey::LastSync => "Última sincronización correcta: {0}.",
        MsgKey::SyncStatus => "Sincronización: {0}",
        MsgKey::SyncExcluded => "{0} ruta(s) excluida(s) por el contrato de sincronización.",
        MsgKey::SyncConflicts => "{0} conflicto(s): {1}. No se sobrescribió nada.",
        MsgKey::SyncConflictCopies => {
            "Una nota cambió en dos dispositivos; se conservaron ambas versiones: {0}."
        }
        MsgKey::SyncWebFailed => "Error de sincronización: {0}",
        MsgKey::BoardNameTaken => "Ya hay un tablero llamado «{0}» en esta carpeta",
        MsgKey::NoNoteNamed => "No hay ninguna nota llamada «{0}» en esta bóveda",
        MsgKey::LinkedNoteHint => "Nota vinculada: {0} · ⌘Enter abre",
        MsgKey::BoardCanvas => "Lienzo del tablero, {0}, herramienta {1}",
        MsgKey::BoardCanvasSelected => {
            "Lienzo del tablero, {0}, herramienta {1}, {2} seleccionado(s)"
        }
        MsgKey::CardLabelEmpty => "{0}, tarjeta vacía",
        MsgKey::CardLabel => "{0}: {1}",
        MsgKey::ThemeLabel => "Tema: {0} — clic para cambiar (Sistema, Oscuro, Claro)",
        MsgKey::AutosaveHelp => {
            "El guardado automático escribe {0} s después de que dejes de teclear."
        }
        MsgKey::Version => "Versión {0}.",
        MsgKey::UpdateAvailable => "Muninn {0} está disponible",
        MsgKey::UpdateInstalled => "Muninn {0} está instalado. Reinicia Muninn para usarlo.",
        MsgKey::UpdateFailed => "Error al actualizar: {0}",
        MsgKey::ShowWithChord => "Mostrar Muninn con {0}",
        MsgKey::ShortcutFailed => "No se pudo registrar {0}: {1}",
        MsgKey::ShortcutOn => "Activado. {0} muestra Muninn desde cualquier aplicación.",
        MsgKey::RelativeToday => "hoy",
        MsgKey::RelativeTomorrow => "mañana",
        MsgKey::RelativeYesterday => "ayer",
        MsgKey::RelativeInDays => "dentro de {0} días",
        MsgKey::RelativeDaysAgo => "hace {0} días",
        MsgKey::DayLabel => "Día {0}",
        MsgKey::RelativeDate => "{0} · {1}",
        MsgKey::NewNoteIn => "Nueva nota en {0}",
        MsgKey::NewFolderIn => "Nueva carpeta en {0}",
        MsgKey::WelcomeVaultCreated => "Nueva bóveda creada en {0} — crea tu primera nota",
        MsgKey::ScanProgress => "Escaneando {0}/{1} notas — Esc para cancelar",
        MsgKey::FocusHelp => "Ocultar la interfaz y mantener centrada la línea del cursor ({0})",
        MsgKey::FocusExitHelp => "Volver a la interfaz de la nota ({0})",
        MsgKey::PaletteKanbanFolder => "Kanban · {0}",
        MsgKey::LinksNotUpdated => "Los enlaces de {0} no se actualizaron",
        MsgKey::RenamedLinksFailed => {
            "Se renombró, pero no se pudieron reescribir {0} archivo(s) con enlaces entrantes: {1}"
        }
        MsgKey::MovedLinksFailed => {
            "Se movió, pero no se pudieron reescribir {0} archivo(s) con enlaces entrantes: {1}"
        }
        MsgKey::NoteNotInVault => "La nota {0} ya no está en esta bóveda",
        MsgKey::CannotOpenWithoutNote => "No se puede abrir {0}: no hay ninguna nota seleccionada",
        MsgKey::AttachmentNotInVault => {
            "El adjunto {0} no está en esta bóveda — arrástralo aquí o corrige la referencia incrustada"
        }
        MsgKey::BrokenLinksCount => "{0} sin resolver",
        MsgKey::NotAFile => "{0} no es un archivo",
        MsgKey::PathGone => "{0} ya no existe",
        MsgKey::TrayError => "No se pudo añadir el icono de la bandeja: {0}",
        MsgKey::LoginItemError => "No se pudo cambiar el elemento de inicio de sesión: {0}",
        MsgKey::OutsideVault => "{0} está fuera de la bóveda",
        MsgKey::NotABoardInVault => "{0} no es un tablero dentro de la bóveda",
        MsgKey::AlreadyExists => "{0} ya existe",
        MsgKey::NotATrashPath => "{0} no es una ruta de la papelera",
        MsgKey::NoLongerInTrash => "{0} ya no está en la papelera",
        MsgKey::NotAFolder => "{0} no es una carpeta",
        MsgKey::BoardNameInvalid => "un nombre de tablero {0}",
        MsgKey::BoardSceneKeyObject => "la escena {0} debe ser un objeto",
        MsgKey::BoardRenamedEmbedsFailed => {
            "Se renombró, pero no se pudieron actualizar {0} nota(s) que lo incrustan: {1}"
        }
        MsgKey::AutostartRegFailed => "No se pudo ejecutar reg.exe: {0}",
        MsgKey::AutostartRegExit => "reg.exe terminó con {0}",
        MsgKey::KanbanQuitRefused => {
            "El kanban no se guardó y no se pudo escribir ninguna copia de recuperación: {0}. Usa Guardar copia o corrige la carpeta y vuelve a salir."
        }
        MsgKey::KanbanCouldNotSaveCopy => "no se pudo guardar una copia: {0}",
        MsgKey::CouldNotDo => "No se pudo {0}: {1}",
        MsgKey::CouldNotCreateBoard => "No se pudo crear un tablero: {0}",
        MsgKey::CouldNotCreateKanban => "No se pudo crear un kanban: {0}",
        MsgKey::CouldNotConvertKanban => "No se pudo convertir en kanban: {0}",
        MsgKey::BoardKeptAsIs => "{0} · {1} sin cambios",
        MsgKey::BoardCouldNotRead => "No se pudo leer: {0}",
        MsgKey::BoardNoteLinkMissing => "{0} (ausente)",
        MsgKey::UnsavedSaves => "Sin guardar · {0} guarda",
        MsgKey::NotSaved => "No se guardó: {0}",
        MsgKey::LinkDescription => "Abrir {0}",
        MsgKey::NoteLabelRoot => "{0} (raíz)",
        MsgKey::GraphNodeParent => "{0} ({1})",
        MsgKey::GraphStatsWithTags => "{0} notas · {1} etiquetas · {2} aristas",
        MsgKey::KanbanFilterFrom => "desde {0}",
        MsgKey::KanbanFilterTo => "hasta {0}",
        MsgKey::KanbanFilterRange => "{0} – {1}",
        MsgKey::LaneActions => "Acciones de la columna: {0}",
        MsgKey::LaneControls => "Controles de {0}",
        MsgKey::CollapseLane => "Contraer la columna: {0}",
        MsgKey::ExpandLane => "Expandir la columna: {0}",
        MsgKey::LaneCards => "Tarjetas de {0}",
        MsgKey::AddCardToLane => "Añadir tarjeta a {0}",
        MsgKey::LaneLabel => "{0}, {1} tarjetas",
        MsgKey::LaneOverLimit => ", límite superado",
        MsgKey::LaneDone => ", columna de completadas",
        MsgKey::LaneRegionLabel => "Columna {0}, {1} tarjetas",
        MsgKey::CollapsedLaneLabel => "{0}, {1} tarjetas, contraída",
        MsgKey::CollapsedCards => "Contraída · {0} tarjetas",
        MsgKey::DeleteLane => {
            "¿Eliminar «{0}»{1}? Cualquier texto de nota que contenga permanece en el archivo."
        }
        MsgKey::DeleteLaneText => {
            "¿Eliminar «{0}»{1}? También se borrará texto de nota que el tablero no muestra ({2} en total), a partir de «{3}»."
        }
        MsgKey::DeleteCardsOne => " y su tarjeta",
        MsgKey::DeleteCardsOther => " y sus {0} tarjetas",
        MsgKey::CardActions => "Acciones de la tarjeta: {0}",
        MsgKey::CardActionsEmpty => "Acciones de la tarjeta: tarjeta vacía",
        MsgKey::CardActionsInLane => "Acciones de la tarjeta: tarjeta vacía en {0}",
        MsgKey::ChecklistProgress => "{0} de {1} completadas",
        MsgKey::ChecklistLabel => "Lista de tareas, {0} de {1} completadas",
        MsgKey::CardLabelInLane => "{0} · {1}: {2}",
        MsgKey::DeviceCode => {
            "Escribe el código {0} en {1}. Tu navegador está abriendo esa página; Muninn termina cuando GitHub lo aprueba."
        }
        MsgKey::BrowserFailed => {
            "No se pudo abrir un navegador. Abre esta dirección tú mismo para continuar: {0}"
        }
        MsgKey::SignInFailed => "El inicio de sesión no terminó: {0} No se guardó ningún token.",
        MsgKey::TailnetHosting => "Alojando en 127.0.0.1:{0}. En este dispositivo ejecuta: {1}",
        MsgKey::TailnetHostingReady => {
            "Alojando. Ejecuta una vez en este dispositivo: {0}. Usa Copiar clave para dar la clave compartida a tus otros dispositivos."
        }
        MsgKey::SharedKeyCopied => {
            "Clave compartida copiada. El portapapeles se vacía en {0} segundos; pégala en el dispositivo que se une."
        }
        MsgKey::SyncPreviousCredential => {
            "No se pudo quitar la credencial de la conexión anterior: {0}"
        }
        MsgKey::SyncNothingSaved => "{0} No se guardó nada.",
        MsgKey::SyncKeyRotationFailed => "{0} La clave anterior sigue funcionando.",
        MsgKey::SyncNowAfterError => "{0} Vuelve a conectarte antes de Sincronizar ahora.",
        MsgKey::SyncOffline => "Sin conexión: {0}",
        MsgKey::SyncRateLimitedAfter => "Demasiadas peticiones; reintenta en {0} ms",
        MsgKey::SyncProviderError => "Error del proveedor {0}: {1}",
        MsgKey::SyncAuthenticationNeeded => "Se necesita autenticación: {0}",
        MsgKey::SyncPermissionDenied => "Permiso denegado: {0}",
        MsgKey::SyncOauthScopeRejected => "Ámbito de OAuth rechazado: {0}",
        MsgKey::SyncRemoteScopeNotFound => "No se encontró el ámbito remoto: {0}",
        MsgKey::SyncRemoteChanged => "El remoto cambió: {0}",
        MsgKey::SyncOutcomeUnknown => "Resultado desconocido; reconcilia antes de reintentar: {0}",
        MsgKey::SyncListingIncomplete => {
            "El listado remoto está incompleto; no se tratará como eliminación: {0}"
        }
        MsgKey::SyncUnsupported => "{0} no es compatible: {1}",
        MsgKey::SyncLimitExceeded => "Se superó el límite de sincronización: {0}",
        MsgKey::SyncCredentialsUnavailable => "Credenciales no disponibles: {0}",
        MsgKey::SyncDirtyBuffer => {
            "Guarda o descarta los cambios sin guardar en {0} antes de que la sincronización pueda modificarlo"
        }
        MsgKey::SyncLocalChanged => {
            "{0} cambió durante la sincronización; vuelve a ejecutar Sincronizar ahora para replanificar"
        }
        MsgKey::SyncNotConfigured => "La sincronización no está configurada: {0}",
        MsgKey::SyncFileError => "Error de archivo: {0}",
        MsgKey::RecoveredFor => "Se recuperaron cambios sin guardar para {0}",
        MsgKey::ImportedFilesOne => "Se importó {0}",
        MsgKey::ImportedFilesOther => "Se importaron {0}",
        MsgKey::ImportJoin => " · {0}",
        MsgKey::ImportAttachmentsNote => " · los archivos que no son Markdown siguen como adjuntos",
    }
}
