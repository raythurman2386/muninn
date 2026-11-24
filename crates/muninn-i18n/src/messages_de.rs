//! The `de-DE` template table for [`MsgKey`].
//!
//! Mirrors [`crate::messages`] arm for arm and in the same order. Every
//! placeholder keeps its position in a natural German sentence; the one/other
//! pairs are separate rows, exactly as in English, so a count of 0 picks the
//! plural row (`0 Notizen`) and 1 picks the singular (`1 Notiz`).
//!
//! The copy is machine-authored and reviewed by a native or fluent speaker
//! before release.

use crate::msg::MsgKey;

/// The German template for `key`.
///
/// Private: callers go through [`crate::phrase`] or [`crate::phrase_current`],
/// which substitute the arguments and pick the language. Keeping every table
/// behind those functions is what makes a second language a change to this
/// crate rather than to call sites.
pub(crate) fn template(key: MsgKey) -> &'static str {
    match key {
        MsgKey::FilesSkippedOne => "{0} Datei übersprungen",
        MsgKey::FilesSkippedOther => "{0} Dateien übersprungen",
        MsgKey::NoteOne => "{0} Notiz",
        MsgKey::NoteOther => "{0} Notizen",
        MsgKey::BoardOne => "{0} Tafel",
        MsgKey::BoardOther => "{0} Tafeln",
        MsgKey::TargetOne => "{0} Ziel",
        MsgKey::TargetOther => "{0} Ziele",
        MsgKey::ReferrerOne => "{0} Verweisquelle",
        MsgKey::ReferrerOther => "{0} Verweisquellen",
        MsgKey::WordOne => "{0} Wort",
        MsgKey::WordOther => "{0} Wörter",
        MsgKey::LinkOne => "{0} Link",
        MsgKey::LinkOther => "{0} Links",
        MsgKey::BacklinkOne => "{0} Backlink",
        MsgKey::BacklinkOther => "{0} Backlinks",
        MsgKey::GraphOutgoingLinkOne => "{0} ausgehender Link",
        MsgKey::GraphOutgoingLinkOther => "{0} ausgehende Links",
        MsgKey::ResultOne => "{0} Ergebnis",
        MsgKey::ResultOther => "{0} Ergebnisse",
        MsgKey::ElementOne => "{0} Element",
        MsgKey::ElementOther => "{0} Elemente",
        MsgKey::ArchivedCardOne => "{0} archivierte Karte",
        MsgKey::ArchivedCardOther => "{0} archivierte Karten",
        MsgKey::LineOne => "{0} Zeile",
        MsgKey::LineOther => "{0} Zeilen",
        MsgKey::FileOne => "{0} Datei",
        MsgKey::FileOther => "{0} Dateien",
        MsgKey::StatusCounts => "{0} · {1} · {2}",
        MsgKey::TrashSummary => "Papierkorb · {0} · {1}",
        MsgKey::NoteLinks => "{0} · {1}",
        MsgKey::NoLinks => "keine Links",
        MsgKey::NoteLinksRoot => "Stammverzeichnis · {0}",
        MsgKey::NoteLinksFolder => "{0} · {1}",
        MsgKey::SidebarSummary => "{0} · {1}",
        MsgKey::GraphLinks => "{0} · {1}",
        MsgKey::RecoveredCopies => "{0} und {1}",
        MsgKey::MoreEdits => "+{0} weitere",
        MsgKey::Deleted => "{0} gelöscht",
        MsgKey::ScanMs => "Scan {0} ms",
        MsgKey::RestoreTo => "Wiederherstellen nach {0}",
        MsgKey::FilterByTag => "Nach #{0} filtern",
        MsgKey::NoteTagFilterSummary => "Notizen mit Tag #{0}",
        MsgKey::LinkImpactOne => "{0} Notiz verlinkt {1}",
        MsgKey::LinkImpactOther => "{0} Notizen verlinken {1}",
        MsgKey::CouldNotSave => "Konnte nicht speichern: {0}",
        MsgKey::CouldNotOpenVault => "Konnte den Tresor nicht öffnen: {0}",
        MsgKey::CouldNotReadVault => "Konnte den Tresor nicht lesen: {0}",
        MsgKey::CouldNotReadNoteState => "Konnte den Notizstatus nicht lesen: {0}",
        MsgKey::CouldNotCreateVault => "Konnte den Tresor nicht erstellen: {0}",
        MsgKey::CouldNotPersistKanbanRecovery => {
            "Konnte die Kanban-Wiederherstellungskopie nicht sichern: {0}"
        }
        MsgKey::CouldNotPersistRecovery => "Konnte die Wiederherstellungskopie nicht sichern: {0}",
        MsgKey::CouldNotPersistBoardRecovery => {
            "Konnte die Tafel-Wiederherstellungskopie nicht sichern: {0}"
        }
        MsgKey::CouldNotReadRecovery => "Konnte die Wiederherstellungskopie nicht lesen: {0}",
        MsgKey::CouldNotRecreateNoteFolder => "Konnte den Notizordner nicht neu erstellen: {0}",
        MsgKey::CouldNotRecreateNote => "Konnte die Notiz nicht neu erstellen: {0}",
        MsgKey::CouldNotReloadNote => "Konnte die Notiz nicht neu laden: {0}",
        MsgKey::CouldNotSaveCopy => "Konnte keine Kopie speichern: {0}",
        MsgKey::CouldNotRenameNote => "Konnte die Notiz nicht umbenennen: {0}",
        MsgKey::CouldNotRenameFolder => "Konnte den Ordner nicht umbenennen: {0}",
        MsgKey::CouldNotDeleteNote => "Konnte die Notiz nicht löschen: {0}",
        MsgKey::CouldNotDeleteFolder => "Konnte den Ordner nicht löschen: {0}",
        MsgKey::CouldNotEmptyTrash => "Konnte den Papierkorb nicht leeren: {0}",
        MsgKey::CouldNotMoveNote => "Konnte die Notiz nicht verschieben: {0}",
        MsgKey::CouldNotCreateFolder => "Konnte den Ordner nicht erstellen: {0}",
        MsgKey::CouldNotCreateNote => "Konnte keine Notiz erstellen: {0}",
        MsgKey::CouldNotAddNoteTemplate => "Konnte die Notizvorlage nicht hinzufügen: {0}",
        MsgKey::CouldNotBeRead => "{0} konnte nicht gelesen werden",
        MsgKey::CouldNotRestore => "Konnte {0} nicht wiederherstellen: {1}",
        MsgKey::CouldNotOpenFile => "Konnte {0} nicht öffnen: {1}. {2}",
        MsgKey::CouldNotRevealFile => "Konnte {0} nicht anzeigen: {1}. {2}",
        MsgKey::CouldNotImportInto => "Konnte nicht nach {0} importieren: {1}",
        MsgKey::RefusingUnsafeRecoveryPath => "Unsicherer Wiederherstellungspfad {0} abgelehnt",
        MsgKey::BoardQuitRefused => {
            "Tafel nicht gespeichert und es konnte keine Wiederherstellungskopie geschrieben werden: {0}. Verwenden Sie „Kopie speichern“ oder beheben Sie den Ordner, und beenden Sie dann erneut."
        }
        MsgKey::ScanCancelled => "Scan nach {0} von {1} Notizen abgebrochen",
        MsgKey::ImportRefused => "Import abgelehnt: {0}",
        MsgKey::ImportPartlyRefused => "{0} von {1} importiert; abgelehnt: {2}",
        MsgKey::CannotImport => {
            "Kann {0} nicht importieren: Legen Sie Markdown-, Bild-, PDF- oder Office-Dokumente ab"
        }
        MsgKey::ImportProgress => "{0} wird importiert…",
        MsgKey::ImportDocumentNotice => "{0} importiert",
        MsgKey::ImportUnsupported => {
            "Konnte {0} nicht importieren: Dieser Dateityp kann nicht konvertiert werden."
        }
        MsgKey::ImportNeedsOcrOne => {
            "Konnte {0} nicht importieren: Seite {1} von {2} ist ein gescanntes Bild und benötigt OCR, das Muninn nicht durchführt. Exportieren Sie zuerst eine Textversion."
        }
        MsgKey::ImportNeedsOcrOther => {
            "Konnte {0} nicht importieren: Seiten {1} von {2} sind gescannte Bilder und benötigen OCR, das Muninn nicht durchführt. Exportieren Sie zuerst eine Textversion."
        }
        MsgKey::ImportEncrypted => {
            "Konnte {0} nicht importieren: Das Dokument ist verschlüsselt. Entfernen Sie das Kennwort und importieren Sie es erneut."
        }
        MsgKey::ImportMalformed => {
            "Konnte {0} nicht importieren: Die Datei ist beschädigt oder kein lesbares Dokument."
        }
        MsgKey::ImportResourceLimit => {
            "Konnte {0} nicht importieren: Das Dokument ist zu komplex zum Konvertieren."
        }
        MsgKey::ImportMissingPart => {
            "Konnte {0} nicht importieren: Dem Dokument fehlt ein benötigter Teil."
        }
        MsgKey::ImportIo => "Konnte {0} nicht importieren: Die Datei konnte nicht gelesen werden.",
        MsgKey::ImportFailed => {
            "Konnte {0} nicht importieren: Das Dokument konnte nicht konvertiert werden."
        }
        MsgKey::ImportTooLarge => {
            "Konnte {0} nicht importieren: Die Datei ist {1} und überschreitet das Limit von {2}."
        }
        MsgKey::ImportBodyTooLarge => {
            "Konnte {0} nicht importieren: Die konvertierte Notiz ist {1} und überschreitet das Limit von {2}."
        }
        MsgKey::RefusedNotAFile => "{0}: keine Datei",
        MsgKey::RefusedUnsupportedType => "{0}: nicht unterstützter Dateityp",
        MsgKey::RefusedAlreadyExists => "{0}: existiert bereits in diesem Ordner",
        MsgKey::ConnectedAs => {
            "Verbunden als {0}. Das Token liegt im Anmeldedatenspeicher des Betriebssystems. Jetzt synchronisieren, wenn bereit."
        }
        MsgKey::LastSync => "Letzte erfolgreiche Synchronisierung {0}.",
        MsgKey::SyncStatus => "Synchronisierung: {0}",
        MsgKey::SyncExcluded => "{0} Pfad(e) durch den Synchronisierungsvertrag ausgeschlossen.",
        MsgKey::SyncConflicts => "{0} Konflikt(e): {1}. Nichts wurde überschrieben.",
        MsgKey::SyncConflictCopies => {
            "Eine Notiz wurde auf zwei Geräten geändert; beide Versionen wurden behalten: {0}."
        }
        MsgKey::BoardNameTaken => "Eine Tafel namens „{0}“ ist bereits in diesem Ordner",
        MsgKey::NoNoteNamed => "Keine Notiz namens „{0}“ in diesem Tresor",
        MsgKey::LinkedNoteHint => "Verknüpfte Notiz: {0} · ⌘Enter öffnet",
        MsgKey::BoardCanvas => "Zeichenfläche, {0}, Werkzeug {1}",
        MsgKey::BoardCanvasSelected => "Zeichenfläche, {0}, Werkzeug {1}, {2} ausgewählt",
        MsgKey::CardLabelEmpty => "{0}, leere Karte",
        MsgKey::CardLabel => "{0}: {1}",
        MsgKey::ThemeLabel => "Design: {0} — klicken zum Wechseln (System, Dunkel, Hell)",
        MsgKey::AutosaveHelp => "Automatisches Speichern schreibt {0} s nach dem Ende der Eingabe.",
        MsgKey::Version => "Version {0}.",
        MsgKey::UpdateAvailable => "Muninn {0} ist verfügbar",
        MsgKey::UpdateInstalled => {
            "Muninn {0} ist installiert. Starten Sie Muninn neu, um es zu verwenden."
        }
        MsgKey::UpdateFailed => "Update fehlgeschlagen: {0}",
        MsgKey::ShowWithChord => "Muninn mit {0} anzeigen",
        MsgKey::ShortcutFailed => "{0} konnte nicht registriert werden: {1}",
        MsgKey::ShortcutOn => "Ein. {0} zeigt Muninn aus jeder App.",
        MsgKey::RelativeToday => "heute",
        MsgKey::RelativeTomorrow => "morgen",
        MsgKey::RelativeYesterday => "gestern",
        MsgKey::RelativeInDays => "in {0} Tagen",
        MsgKey::RelativeDaysAgo => "vor {0} Tagen",
        MsgKey::DayLabel => "Tag {0}",
        MsgKey::RelativeDate => "{0} · {1}",
        MsgKey::NewNoteIn => "Neue Notiz in {0}",
        MsgKey::NewFolderIn => "Neuer Ordner in {0}",
        MsgKey::WelcomeVaultCreated => {
            "Neuer Tresor erstellt unter {0} — erstellen Sie Ihre erste Notiz"
        }
        MsgKey::ScanProgress => "Scan läuft: {0}/{1} Notizen — Esc zum Abbrechen",
        MsgKey::FocusHelp => {
            "Oberfläche ausblenden und die Zeile des Cursors zentriert halten ({0})"
        }
        MsgKey::FocusExitHelp => "Zurück zur Notizoberfläche ({0})",
        MsgKey::PaletteKanbanFolder => "Kanban · {0}",
        MsgKey::LinksNotUpdated => "Links in {0} wurden nicht aktualisiert",
        MsgKey::RenamedLinksFailed => {
            "Umbenannt, aber {0} Datei(en) mit eingehenden Links konnten nicht umgeschrieben werden: {1}"
        }
        MsgKey::MovedLinksFailed => {
            "Verschoben, aber {0} Datei(en) mit eingehenden Links konnten nicht umgeschrieben werden: {1}"
        }
        MsgKey::NoteNotInVault => "Notiz {0} ist nicht mehr in diesem Tresor",
        MsgKey::CannotOpenWithoutNote => "Kann {0} nicht öffnen: Es ist keine Notiz ausgewählt",
        MsgKey::AttachmentNotInVault => {
            "Anhang {0} ist nicht in diesem Tresor — legen Sie ihn ab oder korrigieren Sie die Einbettung"
        }
        MsgKey::BrokenLinksCount => "{0} fehlerhaft",
        MsgKey::NotAFile => "{0} ist keine Datei",
        MsgKey::PathGone => "{0} existiert nicht mehr",
        MsgKey::TrayError => "Das Symbol im Infobereich konnte nicht hinzugefügt werden: {0}",
        MsgKey::LoginItemError => "Das Anmeldeelement konnte nicht geändert werden: {0}",
        MsgKey::OutsideVault => "{0} liegt außerhalb des Tresors",
        MsgKey::NotABoardInVault => "{0} ist keine Tafel innerhalb des Tresors",
        MsgKey::AlreadyExists => "{0} existiert bereits",
        MsgKey::NotATrashPath => "{0} ist kein Papierkorbpfad",
        MsgKey::NoLongerInTrash => "{0} ist nicht mehr im Papierkorb",
        MsgKey::NotAFolder => "{0} ist kein Ordner",
        MsgKey::BoardNameInvalid => "ein Tafelname {0}",
        MsgKey::BoardSceneKeyObject => "{0} in der Szene muss ein Objekt sein",
        MsgKey::BoardRenamedEmbedsFailed => {
            "Umbenannt, aber {0} Notiz(en), die sie einbetten, konnten nicht aktualisiert werden: {1}"
        }
        MsgKey::AutostartRegFailed => "reg.exe konnte nicht ausgeführt werden: {0}",
        MsgKey::AutostartRegExit => "reg.exe wurde mit {0} beendet",
        MsgKey::KanbanQuitRefused => {
            "Kanban nicht gespeichert und es konnte keine Wiederherstellungskopie geschrieben werden: {0}. Verwenden Sie „Kopie speichern“ oder beheben Sie den Ordner, und beenden Sie dann erneut."
        }
        MsgKey::KanbanCouldNotSaveCopy => "konnte keine Kopie speichern: {0}",
        MsgKey::CouldNotDo => "„{0}“ konnte nicht ausgeführt werden: {1}",
        MsgKey::CouldNotCreateBoard => "Konnte keine Tafel erstellen: {0}",
        MsgKey::CouldNotCreateKanban => "Konnte kein Kanban erstellen: {0}",
        MsgKey::CouldNotConvertKanban => "Konnte nicht in ein Kanban umwandeln: {0}",
        MsgKey::BoardKeptAsIs => "{0} · {1} unverändert beibehalten",
        MsgKey::BoardCouldNotRead => "Konnte nicht lesen: {0}",
        MsgKey::BoardNoteLinkMissing => "{0} (fehlt)",
        MsgKey::UnsavedSaves => "Ungespeichert · {0} speichert",
        MsgKey::NotSaved => "Nicht gespeichert: {0}",
        MsgKey::LinkDescription => "{0} öffnen",
        MsgKey::NoteLabelRoot => "{0} (Stammverzeichnis)",
        MsgKey::GraphNodeParent => "{0} ({1})",
        MsgKey::GraphStatsWithTags => "{0} Notizen · {1} Tags · {2} Kanten",
        MsgKey::KanbanFilterFrom => "ab {0}",
        MsgKey::KanbanFilterTo => "bis {0}",
        MsgKey::KanbanFilterRange => "{0} – {1}",
        MsgKey::LaneActions => "Spaltenaktionen: {0}",
        MsgKey::LaneControls => "Steuerelemente für {0}",
        MsgKey::CollapseLane => "Spalte einklappen: {0}",
        MsgKey::ExpandLane => "Spalte ausklappen: {0}",
        MsgKey::LaneCards => "{0} Karten",
        MsgKey::AddCardToLane => "Karte zu {0} hinzufügen",
        MsgKey::LaneLabel => "{0}, {1} Karten",
        MsgKey::LaneOverLimit => ", über dem Limit",
        MsgKey::LaneDone => ", Erledigt-Spalte",
        MsgKey::LaneRegionLabel => "Spalte {0}, {1} Karten",
        MsgKey::CollapsedLaneLabel => "{0}, {1} Karten, eingeklappt",
        MsgKey::CollapsedCards => "Eingeklappt · {0} Karten",
        MsgKey::DeleteLane => "„{0}“{1} löschen? Jeder Notiztext darin bleibt in der Datei.",
        MsgKey::DeleteLaneText => {
            "„{0}“{1} löschen? Dabei wird auch Notiztext gelöscht, den die Tafel nicht anzeigt ({2}), beginnend mit „{3}“."
        }
        MsgKey::DeleteCardsOne => " und ihre Karte",
        MsgKey::DeleteCardsOther => " und ihre {0} Karten",
        MsgKey::CardActions => "Kartenaktionen: {0}",
        MsgKey::CardActionsEmpty => "Kartenaktionen: leere Karte",
        MsgKey::CardActionsInLane => "Kartenaktionen: leere Karte in {0}",
        MsgKey::ChecklistProgress => "{0} von {1} erledigt",
        MsgKey::ChecklistLabel => "Checkliste, {0} von {1} erledigt",
        MsgKey::CardLabelInLane => "{0} · {1}: {2}",
        MsgKey::DeviceCode => {
            "Geben Sie den Code {0} unter {1} ein. Ihr Browser öffnet die Seite bereits; Muninn schließt den Vorgang ab, sobald GitHub zustimmt."
        }
        MsgKey::BrowserFailed => {
            "Konnte keinen Browser öffnen. Öffnen Sie diese Adresse selbst, um fortzufahren: {0}"
        }
        MsgKey::SignInFailed => {
            "Anmeldung nicht abgeschlossen: {0} Es wurde kein Token gespeichert."
        }
        MsgKey::TailnetHosting => {
            "Wird auf 127.0.0.1:{0} gehostet. Führen Sie auf diesem Gerät aus: {1}"
        }
        MsgKey::TailnetHostingReady => {
            "Wird gehostet. Führen Sie einmal auf diesem Gerät aus: {0}. Verwenden Sie Schlüssel kopieren, um den gemeinsamen Schlüssel an Ihre anderen Geräte weiterzugeben."
        }
        MsgKey::SharedKeyCopied => {
            "Gemeinsamer Schlüssel kopiert. Die Zwischenablage wird in {0} Sekunden geleert; fügen Sie ihn auf dem beitretenden Gerät ein."
        }
        MsgKey::SyncPreviousCredential => {
            "Die Anmeldedaten der vorherigen Verbindung konnten nicht entfernt werden: {0}"
        }
        MsgKey::SyncNothingSaved => "{0} Es wurde nichts gespeichert.",
        MsgKey::SyncKeyRotationFailed => "{0} Der alte Schlüssel funktioniert weiterhin.",
        MsgKey::SyncNowAfterError => {
            "{0} Verbinden Sie erneut, bevor Sie „Jetzt synchronisieren“ wählen."
        }
        MsgKey::SyncOffline => "Offline: {0}",
        MsgKey::SyncRateLimitedAfter => "Zu viele Anfragen; erneut versuchen nach {0} ms",
        MsgKey::SyncProviderError => "Anbieterfehler {0}: {1}",
        MsgKey::SyncAuthenticationNeeded => "Authentifizierung erforderlich: {0}",
        MsgKey::SyncPermissionDenied => "Zugriff verweigert: {0}",
        MsgKey::SyncOauthScopeRejected => "OAuth-Berechtigung abgelehnt: {0}",
        MsgKey::SyncRemoteScopeNotFound => "Berechtigungsbereich im Remote nicht gefunden: {0}",
        MsgKey::SyncRemoteChanged => "Remote geändert: {0}",
        MsgKey::SyncOutcomeUnknown => {
            "Ergebnis unbekannt, vor dem erneuten Versuch abgleichen: {0}"
        }
        MsgKey::SyncListingIncomplete => {
            "Remote-Auflistung unvollständig; wird nicht als Löschung behandelt: {0}"
        }
        MsgKey::SyncUnsupported => "{0} wird nicht unterstützt: {1}",
        MsgKey::SyncLimitExceeded => "Synchronisierungslimit überschritten: {0}",
        MsgKey::SyncCredentialsUnavailable => "Anmeldedaten nicht verfügbar: {0}",
        MsgKey::SyncDirtyBuffer => {
            "Speichern oder verwerfen Sie ungespeicherte Änderungen in {0}, bevor die Synchronisierung sie ändern kann"
        }
        MsgKey::SyncLocalChanged => {
            "{0} wurde während der Synchronisierung geändert; führen Sie „Jetzt synchronisieren“ erneut aus, um neu zu planen"
        }
        MsgKey::SyncNotConfigured => "Die Synchronisierung ist nicht konfiguriert: {0}",
        MsgKey::SyncFileError => "Dateifehler: {0}",
        MsgKey::RecoveredFor => "Ungespeicherte Änderungen für {0} wiederhergestellt",
        MsgKey::ImportedFilesOne => "{0} importiert",
        MsgKey::ImportedFilesOther => "{0} importiert",
        MsgKey::ImportJoin => " · {0}",
        MsgKey::ImportAttachmentsNote => " · Dateien, die kein Markdown sind, bleiben Anhänge",
    }
}
