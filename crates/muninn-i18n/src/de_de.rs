//! The `de-DE` catalog.
//!
//! German (Germany), neutral software register. This exhaustive `match`
//! mirrors [`crate::en_us`] arm for arm and in the same order, so a key added
//! to the catalog fails `cargo check` here until it has German text.
//! Identifiers, keycaps, product names and on-disk tokens stay as written; see
//! `docs/i18n.md` §5.8.
//!
//! The copy is machine-authored and reviewed by a native or fluent speaker
//! before release. The choices that need care are recorded in
//! `tests/german.rs` (the invariant allowlist) and in the pull request's
//! terminology notes.

use crate::key::Key;

/// The German text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping every catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Karte hinzufügen",
        Key::Archive => "Archivieren",
        Key::Today => "Heute",
        Key::OverLimit => "Limit überschritten",
        Key::KeyboardShortcuts => "Tastenkürzel",
        Key::Close => "Schließen",
        Key::Save => "Speichern",
        Key::Cancel => "Abbrechen",
        Key::Clear => "Leeren",
        Key::LinkedNote => "Verknüpfte Notiz",
        Key::NoBoards => "Noch keine Tafeln in diesem Tresor",
        Key::HelpLastLogLines => "letzte Protokollzeilen:",
        Key::HelpGroupNavigation => "Navigation",
        Key::HelpNavFocusSearch => "Suche fokussieren",
        Key::HelpNavMoveHighlightedResult => "Markiertes Ergebnis verschieben",
        Key::HelpNavOpenHighlightedResult => "Markiertes Ergebnis öffnen",
        Key::HelpNavBackInNoteHistory => "Zurück im Notizverlauf",
        Key::HelpNavForwardInNoteHistory => "Vorwärts im Notizverlauf",
        Key::HelpNavWriteReadGraphBoardKanban => "Schreiben / Lesen / Graph / Tafel / Kanban",
        Key::HelpNavFocusMode => "Fokusmodus: Oberfläche ausblenden und Cursor zentrieren",
        Key::HelpNavCloseOutermostLayer => "Äußerste Ebene schließen",
        Key::HelpNavOpenCommandPalette => "Befehlspalette öffnen",
        Key::HelpGroupEdit => "Bearbeiten",
        Key::HelpEditFindInOpenNote => "In der geöffneten Notiz suchen",
        Key::HelpEditFindInBoard => "In der Tafel suchen",
        Key::HelpEditHideCardsByTextTagDateOrState => {
            "Karten nach Text, Etikett, Datum oder Status ausblenden"
        }
        Key::HelpEditPerBoardOverrides => "Tafelweise Überschreibungen, jeweils ein Schlüssel",
        Key::HelpEditInsertWikilink => "Wikilink einfügen",
        Key::HelpEditNewNote => "Neue Notiz",
        Key::HelpEditImportDocument => "Dokument als Notiz importieren",
        Key::HelpEditSaveNow => "Jetzt speichern (automatisches Speichern ist aktiv)",
        Key::HelpEditToggleWriteRead => "Zwischen Schreiben / Lesen wechseln",
        Key::HelpEditWikilinkCompletion => "Wikilink-Vervollständigung",
        Key::HelpEditVaultTagCompletion => "Etikett-Vervollständigung im Tresor",
        Key::HelpEditIndentFourSpaces => "Vier Leerzeichen einrücken",
        Key::HelpEditPasteRichText => {
            "Formatierter Text wird zu Markdown; eine URL über einer Auswahl wird zum Link"
        }
        Key::HelpEditRenameSelectedNote => {
            "Ausgewählte Notiz umbenennen (auf der Tafel: die Tafel)"
        }
        Key::HelpEditDeleteSelectedNote => "Ausgewählte Notiz löschen",
        Key::HelpGroupKanban => "Kanban",
        Key::HelpKanbanMoveThroughControls => "Durch die Steuerelemente der Tafel navigieren",
        Key::HelpKanbanMoveBetweenLaneCards => {
            "Zwischen den Karten der fokussierten Spalte wechseln"
        }
        Key::HelpKanbanMoveSelectionAcrossLanes => "Auswahl über Spalten verschieben",
        Key::HelpKanbanToggleCardDone => "Erledigt-Markierung der ausgewählten Karte umschalten",
        Key::HelpKanbanMoveSelectedCard => "Ausgewählte Karte verschieben",
        Key::HelpKanbanMoveCardToEdgeOrLane => {
            "Nach oben, unten oder in eine andere Spalte verschieben"
        }
        Key::HelpKanbanOpenCardMenu => "Menü der fokussierten Karte öffnen",
        Key::HelpKanbanEditCard => "Karte bearbeiten",
        Key::HelpKanbanCloseMenuOrConfirmation => "Menü, Bestätigung oder Konfliktleiste schließen",
        Key::HelpGroupGraph => "Graph",
        Key::HelpGraphToggleGraph => "Graph umschalten",
        Key::HelpGraphToggleGlobalLocal => "Zwischen Global / Lokal wechseln",
        Key::HelpGraphMoveFocusRing => "Fokusring verschieben",
        Key::HelpGraphOpenFocusedNote => "Fokussierte Notiz öffnen",
        Key::HelpGraphZoomInOut => "Vergrößern / verkleinern",
        Key::HelpGraphFitVisibleNodes => "Sichtbare Knoten einpassen",
        Key::HelpGraphToggleIsolate => "Isolieren umschalten",
        Key::HelpGraphLeaveGraph => "Graph verlassen",
        Key::HelpGroupBoard => "Tafel",
        Key::HelpBoardPanWithDrag => "Tafel verschieben (oder mit der Hand ziehen)",
        Key::HelpBoardZoomAtPointer => "Am Zeiger zoomen",
        Key::HelpBoardZoomAtCanvasCentre => "In der Zeichenflächenmitte vergrößern / verkleinern",
        Key::HelpBoardPanTheBoard => "Tafel verschieben",
        Key::HelpBoardFitTheDrawing => "Zeichnung einpassen",
        Key::HelpBoardShapeTools => {
            "Werkzeug für Rechteck, Raute, Ellipse, Pfeil, Linie oder Freihand"
        }
        Key::HelpBoardDrawShape => "Form zeichnen; das Werkzeug bleibt ausgewählt",
        Key::HelpBoardPlaceText => "Text platzieren, dann direkt eintippen",
        Key::HelpBoardEditTextLabel => "Textbeschriftung schreiben oder bearbeiten",
        Key::HelpBoardDropTheTool => "Werkzeug ablegen (zurück zur Auswahl)",
        Key::HelpBoardSelectElementOrToggle => "Element auswählen / hinzufügen oder entfernen",
        Key::HelpBoardNudgeSelection => "Auswahl um 1 Einheit verschieben (Shift: 5)",
        Key::HelpBoardDeleteSelection => "Auswahl löschen",
        Key::HelpBoardSelectEveryElement => "Alle Elemente auswählen",
        Key::HelpBoardLinkToNote => {
            "Ausgewählte Elemente mit einer Notiz verknüpfen (leer hebt die Verknüpfung auf)"
        }
        Key::HelpBoardOpenLinkedNote => "Verknüpfte Notiz des ausgewählten Elements öffnen",
        Key::HelpBoardOpenClickedLinkedNote => {
            "Verknüpfte Notiz des angeklickten Elements öffnen (ein Klick allein wählt aus)"
        }
        Key::HelpBoardUndoRedo => {
            "Zeichnen, Verschieben und Löschen rückgängig machen / wiederholen"
        }
        Key::HelpBoardSaveDrawing => "Zeichnung der Tafel speichern",
        Key::HelpBoardEscapeClearsSelection => {
            "Auswahl aufheben, sonst Werkzeug ablegen, sonst Tafel verlassen"
        }
        Key::HelpGroupSidebar => "Seitenleiste",
        Key::HelpSidebarOpenThisHelp => "Diese Hilfe öffnen",
        Key::HelpSidebarOpenHelpWhenNotTyping => "Hilfe öffnen, wenn nicht getippt wird",
        Key::HelpSidebarMoveBetweenPanes => "Zwischen Bereichen wechseln",
        Key::HelpSidebarMoveThroughAList => "Durch eine Liste navigieren",
        Key::HelpSidebarOpenFocusedRow => "Fokussierte Zeile öffnen",
        Key::HelpSidebarNoteAndFolderActions => "Aktionen für Notizen und Ordner",
        Key::HelpSidebarSettings => {
            "Einstellungen (Tresore, Design, Graph-Effekte, Kanban-Standardwerte)"
        }
        Key::SettingsTabVaults => "Tresore",
        Key::SettingsTabEditor => "Editor",
        Key::SettingsTabAppearance => "Darstellung",
        Key::SettingsTabGraph => "Graph",
        Key::SettingsTabSync => "Synchronisierung",
        Key::SettingsTabKanban => "Kanban",
        Key::SettingsVaultsOpenTitle => "Tresor öffnen",
        Key::SettingsVaultsOpenCaption => {
            "Ordnerpfad einfügen oder in einem neuen Ordner einen Tresor erstellen."
        }
        Key::SettingsVaultsRecentTitle => "Zuletzt verwendete Tresore",
        Key::SettingsVaultsRecentCaption => {
            "Tresore, die Sie zuletzt geöffnet haben, der neueste zuerst."
        }
        Key::SettingsVaultsRecentEmpty => "Noch keine zuletzt verwendeten Tresore",
        Key::SettingsVaultsPathLabel => "Pfad des Tresorordners",
        Key::SettingsVaultsPathHint => "Ordnerpfad einfügen; ~ wird zum Benutzerordner erweitert.",
        Key::SettingsVaultsOpenAction => "Tresor öffnen",
        Key::SettingsVaultsNewAction => "Neuer Tresor",
        Key::SettingsEditorAutosaveTitle => "Verzögerung beim automatischen Speichern",
        Key::SettingsEditorAutosaveCaption => {
            "Sekunden ohne Eingabe, bevor die geöffnete Notiz geschrieben wird."
        }
        Key::SettingsEditorTemplateTitle => "Standard-Notizvorlage",
        Key::SettingsEditorTemplateCaption => {
            "Markdown, das oben in jede neue Notiz eingefügt wird. Leer lassen für eine reine Überschrift."
        }
        Key::SettingsEditorTemplateLabel => "Standard-Notizvorlage",
        Key::SettingsEditorTemplatePlaceholder => {
            "Markdown, das oben in jede neue Notiz eingefügt wird"
        }
        Key::SettingsAppearanceThemeTitle => "Design",
        Key::SettingsAppearanceThemeFollowing => "Folgt dem Omarchy-Systemdesign",
        Key::SettingsAppearanceLanguageTitle => "Sprache",
        Key::SettingsAppearanceLanguageCaption => {
            "System folgt der Sprache des Betriebssystems und fällt auf Englisch zurück, wenn kein Sprachpaket passt."
        }
        Key::SettingsAppearanceLanguageSystem => "System",
        Key::SettingsAppearanceUnavailable => "Unter macOS noch nicht verfügbar.",
        Key::SettingsAppearanceStateOn => "Ein.",
        Key::SettingsAppearanceStateOff => "Aus.",
        Key::SettingsAppearanceTrayTitle => "Infobereichssymbol",
        Key::SettingsAppearanceTrayCaption => {
            "Ein Symbol im Infobereich mit „Muninn anzeigen“, „Neue Notiz“, „Jetzt synchronisieren“, dem Synchronisierungsstatus und „Beenden“."
        }
        Key::SettingsAppearanceTrayShow => "Symbol im Infobereich anzeigen",
        Key::SettingsAppearanceTrayStatusOn => {
            "Ein. Linux zeigt es, wo ein Infobereich läuft; GNOME benötigt eine AppIndicator-Erweiterung."
        }
        Key::SettingsAppearanceCloseToTrayTitle => "In den Infobereich schließen",
        Key::SettingsAppearanceCloseToTrayNeedsTray => {
            "Benötigt das Symbol im Infobereich. Das Schließen des Fensters beendet Muninn."
        }
        Key::SettingsAppearanceCloseToTrayOff => "Aus. Das Schließen des Fensters beendet Muninn.",
        Key::SettingsAppearanceCloseToTrayOn => {
            "Ein. Das Schließen des Fensters lässt Muninn im Infobereich; „Muninn beenden“ im Infobereich beendet Muninn. Wenn kein Tray-Host das Symbol anzeigt, lassen Sie dies aus."
        }
        Key::SettingsAppearanceCloseToTrayNoIcon => {
            "Ein, aber derzeit gibt es kein Symbol im Infobereich, daher beendet das Schließen des Fensters Muninn."
        }
        Key::SettingsAppearanceStartupTitle => "Start und Tastenkürzel",
        Key::SettingsAppearanceLoginTitle => "Muninn bei der Anmeldung starten",
        Key::SettingsAppearanceLoginOn => "Ein. Muninn öffnet sich bei der Anmeldung.",
        Key::SettingsAppearanceShortcutRegistering => "Wird registriert…",
        Key::SettingsGraphReduceMotionTitle => "Bewegung reduzieren",
        Key::SettingsGraphReduceMotionCaption => {
            "Friert das Graph-Layout ein, überspringt Kamerafahrten und macht Hover-, Auswahl- und Graph-Effekte statisch. System folgt Ihrer Betriebssystem-Einstellung."
        }
        Key::SettingsGraphReduceMotionEnv => {
            "Derzeit ein: MUNINN_REDUCED_MOTION ist gesetzt und überschreibt diese Wahl."
        }
        Key::SettingsGraphReduceMotionSystemOn => "Derzeit ein, der Systemeinstellung folgend.",
        Key::SettingsGraphReduceMotionSystemOff => "Derzeit aus, der Systemeinstellung folgend.",
        Key::SettingsGraphReduceMotionOn => "Derzeit ein.",
        Key::SettingsGraphReduceMotionOff => "Derzeit aus.",
        Key::SettingsGraphEffectsTitle => "Graph-Effekte",
        Key::SettingsGraphEffectsCaption => {
            "Dieselbe Effektfamilie zeichnet den Rand der Auswahl und die Linien, die sie berührt: Klassisch behält die heutigen schlichten Linien, Elektrisch zeichnet beides als Bögen, Aus zeichnet schlichte Linien ohne Effektebenen. Reduzierte Bewegung friert ein, was Sie wählen."
        }
        Key::SettingsGraphIntensityTitle => "Effektstärke",
        Key::SettingsGraphIntensityCaption => {
            "Skaliert die elektrischen Bögen und das Löschbrennen. Ausgewogen ist das ursprüngliche Aussehen. Klassisch zeichnet keine Bögen und Aus zeichnet nichts, daher ist die Bogenwahl nur bei Elektrisch sichtbar; das Brennen skaliert weiterhin bei beiden. Reduzierte Bewegung friert das Ergebnis weiterhin ein."
        }
        Key::SyncHeadingProvider => "Synchronisierungsanbieter",
        Key::SyncHeadingProviderCaption => {
            "Nur manuell über Jetzt synchronisieren. Ein Anbieter für diesen Tresor. Trennen entfernt Anmeldedaten, nicht Notizen."
        }
        Key::SyncProviderGithubName => "GitHub",
        Key::SyncProviderGithubDescription => {
            "GitHub ausgewählt. Verwenden Sie ein eigenes Repository und einen eigenen Branch, nicht das Muninn-Quellrepository."
        }
        Key::SyncProviderDriveName => "Google Drive",
        Key::SyncProviderDriveShortName => "Drive",
        Key::SyncProviderDriveDescription => {
            "Google Drive ausgewählt. Die Synchronisierung hält eine app-verwaltete Commit-Historie in Muninns privaten App-Daten in Ihrem Drive, nicht in bearbeitbaren Markdown-Dateien."
        }
        Key::SyncProviderDriveCaption => {
            "In Muninns privaten App-Daten in Ihrem Drive, nicht in einem sichtbaren Ordner. Verwenden Sie auf jedem Gerät denselben Speichernamen."
        }
        Key::SyncProviderTailscaleName => "Tailscale",
        Key::SyncProviderTailscaleDescription => {
            "Tailscale ausgewählt. Ein Gerät hostet einen Commit-Speicher; die anderen treten ihm über den MagicDNS-Namen in Ihrem Tailnet bei."
        }
        Key::SyncFieldGithubOwner => "GitHub-Besitzer",
        Key::SyncFieldRepository => "Repository",
        Key::SyncFieldBranch => "Branch",
        Key::SyncFieldPrefix => "Präfix",
        Key::SyncFieldGithubClientId => "GitHub-Client-ID (optional)",
        Key::SyncFieldStoreName => "Speichername",
        Key::SyncFieldGoogleClientId => "Google-Client-ID (optional)",
        Key::SyncFieldLoopbackPort => "Loopback-Port",
        Key::SyncFieldAllowedLogins => {
            "Erlaubte Tailscale-Anmeldungen (optional, durch Kommas getrennt)"
        }
        Key::SyncFieldMagicDnsName => "MagicDNS-Name des Hosts (device.tailnet.ts.net)",
        Key::SyncFieldSharedKey => "Gemeinsamer Schlüssel vom Host",
        Key::SyncActionConnect => "Verbinden",
        Key::SyncActionSyncNow => "Jetzt synchronisieren",
        Key::SyncActionDisconnect => "Trennen",
        Key::SyncTailnetRoleLabel => "Tailscale-Rolle",
        Key::SyncTailnetRoleHost => "Hier hosten",
        Key::SyncTailnetRoleJoin => "Host beitreten",
        Key::SyncTailnetKeyLabel => "Gemeinsamer Schlüssel",
        Key::SyncTailnetKeyCopy => "Schlüssel kopieren",
        Key::SyncTailnetKeyShow => "Schlüssel anzeigen",
        Key::SyncTailnetKeyHide => "Schlüssel verbergen",
        Key::SyncTailnetKeyRotate => "Schlüssel erneuern",
        Key::SyncStatusNotConnected => {
            "Nicht verbunden. Wählen Sie einen Anbieter und ein explizites Remote und wählen Sie dann Verbinden."
        }
        Key::SyncStatusTailnetHost => {
            "Dieses Gerät hostet. Muninn stellt den Commit-Speicher auf 127.0.0.1 bereit, solange es geöffnet ist; tailscale serve veröffentlicht ihn in Ihrem Tailnet."
        }
        Key::SyncStatusTailnetJoin => {
            "Einem Host beitreten. Geben Sie den vollständigen MagicDNS-Namen des Hosts ein und fügen Sie den Schlüssel ein, den „Schlüssel kopieren“ auf dem Host liefert."
        }
        Key::SyncStatusConnectedSharedKey => {
            "Verbunden. Der gemeinsame Schlüssel liegt im Anmeldedatenspeicher des Betriebssystems. Jetzt synchronisieren, wenn bereit."
        }
        Key::SyncStatusConnectedToken => {
            "Verbunden. Das Token liegt im Anmeldedatenspeicher des Betriebssystems. Jetzt synchronisieren, wenn bereit."
        }
        Key::SyncStatusAskingGithubCode => "Anmeldecode wird bei GitHub angefordert…",
        Key::SyncStatusOpeningGoogle => "Google-Anmeldung wird im Browser geöffnet…",
        Key::SyncStatusWaitingGoogle => {
            "Schließen Sie die Google-Anmeldung im Browser ab. Muninn wartet auf 127.0.0.1 auf die Antwort."
        }
        Key::SyncStatusSignedInAfterDisconnect => {
            "Anmeldung nach dem Trennen abgeschlossen; das Token wurde entfernt."
        }
        Key::SyncStatusTailnetJoined => {
            "Beigetreten. Der gemeinsame Schlüssel liegt im Anmeldedatenspeicher des Betriebssystems. Über Jetzt synchronisieren erreichen Sie den Host in Ihrem Tailnet."
        }
        Key::SyncStatusSyncStarted => "Synchronisierung gestartet. Das Fenster bleibt bedienbar.",
        Key::SyncStatusCancelling => {
            "Wird abgebrochen. Eine Änderung, die der Anbieter bereits angenommen hat, bleibt erhalten und wird abgeglichen."
        }
        Key::SyncStatusNoteChangedWhileSyncing => {
            "Die geöffnete Notiz hat sich während der Synchronisierung geändert; ihr Text wurde nicht überschrieben."
        }
        Key::SyncStatusNothingConnected => {
            "Es ist nichts verbunden. Lokale Notizen wurden nicht geändert."
        }
        Key::SyncStatusDisconnected => {
            "Getrennt. Anmeldedaten für diese Bindung wurden entfernt. Lokale und entfernte Notizen wurden nicht gelöscht."
        }
        Key::SyncStatusSaveBeforeSync => {
            "Speichern oder klären Sie die geöffnete Notiz, bevor Sie „Jetzt synchronisieren“ wählen. Ungespeicherter Text wird nicht hochgeladen oder überschrieben."
        }
        Key::SyncStatusAlreadyRunning => "Eine Synchronisierung läuft bereits.",
        Key::SyncStatusFinishSignIn => {
            "Schließen Sie zuerst die Anmeldung ab oder wählen Sie Trennen, um abzubrechen."
        }
        Key::SyncStatusConnectProvider => {
            "Verbinden Sie einen Anbieter für diesen Tresor, bevor Sie synchronisieren."
        }
        Key::SyncStatusWaitForRun => {
            "Warten Sie, bis die laufende Synchronisierung oder Anmeldung abgeschlossen ist, oder wählen Sie Trennen, um sie abzubrechen."
        }
        Key::SyncStatusWaitToRotateKey => {
            "Warten Sie, bis die laufende Synchronisierung abgeschlossen ist, bevor Sie den Schlüssel erneuern."
        }
        Key::SyncStatusKeyRotated => {
            "Schlüssel erneuert; der alte Schlüssel funktioniert nicht mehr. Wählen Sie auf jedem beigetretenen Gerät Trennen und treten Sie mit dem neuen Schlüssel erneut bei (Schlüssel kopieren)."
        }
        Key::SyncStatusCurrentConnectionKept => "Die aktuelle Verbindung wurde nicht geändert.",
        Key::SyncErrorVaultNotCanonical => {
            "Der Pfad dieses Tresorordners kann nicht kanonisiert werden, daher kann er nicht gebunden werden."
        }
        Key::SyncErrorGithubScope => {
            "Geben Sie vor dem Verbinden den GitHub-Besitzer und das Repository ein."
        }
        Key::SyncErrorLoopbackPort => {
            "Geben Sie einen Loopback-Port zwischen 1024 und 65535 für den Host ein."
        }
        Key::SyncErrorPasteSharedKey => {
            "Fügen Sie den auf dem Host angezeigten gemeinsamen Schlüssel ein. Er wird im Anmeldedatenspeicher des Betriebssystems gespeichert, nicht in den Einstellungen."
        }
        Key::SyncErrorNotTailnetHost => "Dieser Tresor hostet keinen Tailscale-Speicher.",
        Key::SyncErrorSharedKeyMissing => "Der gemeinsame Schlüssel fehlt im Anmeldedatenspeicher.",
        Key::SyncErrorTailnetKeyMissing => {
            "Der gemeinsame Tailscale-Schlüssel fehlt im Anmeldedatenspeicher. Trennen und erneut verbinden."
        }
        Key::SyncErrorCancelled => "Synchronisierung abgebrochen",
        Key::SyncErrorTimedOutAfterSend => {
            "Zeitüberschreitung, nachdem eine Änderung gesendet wurde; vor dem erneuten Versuch abgleichen"
        }
        Key::SyncErrorTimedOutBeforeResponse => {
            "Zeitüberschreitung, bevor der Anbieter geantwortet hat"
        }
        Key::SyncErrorRateLimited => "Zu viele Anfragen",
        Key::SyncErrorLeaseBusy => "Eine andere Muninn-Instanz schreibt diesen Tresor",
        Key::SyncErrorSettingsNotObject => {
            "Die Synchronisierungseinstellungen waren kein Objekt, daher ist die Synchronisierung aus."
        }
        Key::SyncErrorSettingsVersion => {
            "Die Version der Synchronisierungseinstellungen wird nicht unterstützt, daher ist die Synchronisierung aus."
        }
        Key::SyncErrorSettingsVaultList => {
            "Die Tresorliste für die Synchronisierung fehlte, daher ist die Synchronisierung aus."
        }
        Key::SyncErrorSettingsBindingInvalid => {
            "Eine Synchronisierungsbindung war ungültig und wurde nicht geladen. Die Synchronisierung ist aus, bis sie ersetzt wird."
        }
        Key::KanbanUiNotInsideVault => "das Kanban liegt nicht im geöffneten Tresor",
        Key::KanbanUiThis => "das Kanban",
        Key::KanbanUiFieldRefused => {
            "Kein Beenden: das Kanban konnte nicht gespeichert werden. Beheben Sie, was das Speichern verhindert hat, oder machen Sie die Änderungen rückgängig, und beenden Sie dann erneut."
        }
        Key::ErrorSaveKanbanBeforeRestore => {
            "Speichern oder verwerfen Sie das aktuelle Kanban, bevor Sie ein anderes wiederherstellen"
        }
        Key::PaletteTitle => "Befehlspalette",
        Key::PaletteEmpty => "Keine Notizen oder Aktionen entsprechen der Suche",
        Key::PaletteFooter => "↑↓ bewegen · Enter öffnen · Esc schließen",
        Key::PaletteActionOpenLocalGraph => "Lokalen Graphen öffnen",
        Key::PaletteActionSwitchVault => "Tresor wechseln",
        Key::PaletteActionOpenSettings => "Einstellungen öffnen",
        Key::PaletteActionNewNote => "Neue Notiz",
        Key::PaletteActionTogglePreview => "Zwischen Schreiben / Lesen wechseln",
        Key::PaletteActionToggleGraph => "Graph umschalten",
        Key::PaletteActionToggleBoard => "Tafel umschalten",
        Key::PaletteActionToggleKanban => "Kanban umschalten",
        Key::PaletteActionToggleFocusMode => "Fokusmodus umschalten",
        Key::PaletteActionNewKanban => "Neues Kanban",
        Key::PaletteActionConvertToKanban => "Leere Notiz in Kanban umwandeln",
        Key::PaletteActionImportDocument => "Dokument importieren…",
        Key::PaletteHintGraphLocal => "Graph · Lokal",
        Key::PaletteHintVaults => "Tresore",
        Key::PaletteHintHelp => "Hilfe",
        Key::PaletteHintKanban => "Kanban",
        Key::PaletteKeywordsOpenLocalGraph => "graph lokal fokus nachbarn umfang",
        Key::PaletteKeywordsSwitchVault => "tresor öffnen zuletzt ordner wechseln",
        Key::PaletteKeywordsOpenSettings => "einstellungen optionen konfiguration hilfe",
        Key::PaletteKeywordsNewNote => "neu notiz erstellen hinzufügen",
        Key::PaletteKeywordsTogglePreview => "schreiben lesen vorschau editor modus",
        Key::PaletteKeywordsToggleGraph => "graph global lokal modus",
        Key::PaletteKeywordsToggleBoard => {
            "tafel whiteboard excalidraw zeichnung leinwand skizze modus"
        }
        Key::PaletteKeywordsToggleKanban => "kanban spalten karten aufgaben modus",
        Key::PaletteKeywordsToggleFocusMode => {
            "fokus zen ablenkungsfrei schreiben konzentrieren oberfläche ausblenden"
        }
        Key::PaletteKeywordsNewKanban => "kanban erstellen hinzufügen spalten karten aufgaben",
        Key::PaletteKeywordsConvertToKanban => "kanban leer umwandeln spalten karten aufgaben",
        Key::PaletteKeywordsImportDocument => {
            "import dokument docx doc word pdf office odt rtf epub csv"
        }
        Key::PaletteSectionRecent => "Zuletzt",
        Key::PaletteSectionNotes => "Notizen",
        Key::PaletteSectionActions => "Aktionen",
        Key::PaletteSectionKanbans => "Kanbans",
        Key::BoardPageTitle => "Tafeln",
        Key::BoardPageNew => "Neue Tafel",
        Key::BoardPageEmptyTitle => "Noch keine Tafeln",
        Key::BoardPageEmptyDetail => "Erstellen Sie eine über „Neue Tafel“",
        Key::BoardPageEmptyHint => {
            "Eine Tafel ist eine Excalidraw-Datei in Ihrem Tresor. Erstellen Sie eine, um mit dem Zeichnen zu beginnen."
        }
        Key::BoardPageNoSelectionTitle => "Keine Tafel ausgewählt",
        Key::BoardPageNoSelectionDetail => {
            "Wählen Sie oben eine Tafel aus der Auswahl, um zu sehen, was sie enthält"
        }
        Key::BoardPageTitleLabel => "Tafelname; Doppelklick zum Umbenennen",
        Key::BoardPageUnreadableTitle => "Diese Tafel konnte nicht gelesen werden",
        Key::BoardPageNoOpenTitle => "Keine Tafel geöffnet",
        Key::BoardPageNoOpenDetail => {
            "Wählen Sie oben eine Tafel oder erstellen Sie eine neue. Tafeln sind Excalidraw-Dateien in Ihrem Tresor."
        }
        Key::BoardPageEmptyCanvas => "Auf dieser Tafel ist noch nichts gezeichnet",
        Key::BoardPageHint => {
            "Klicken/Ziehen auswählen · Entf löschen · ⌘Z rückgängig · R D O A L P zeichnen · H verschieben · ⌘L Notiz verknüpfen · ⌘S speichern · 0 einpassen"
        }
        Key::BoardPagePickerLabel => "Tafel auswählen",
        Key::BoardPagePickerHelp => "Tafelliste öffnen",
        Key::BoardPagePickerMenu => "Tafeln",
        Key::BoardToolToolbar => "Tafelwerkzeuge",
        Key::BoardToolHand => "Hand",
        Key::BoardToolHandHelp => "Tafel verschieben (H)",
        Key::BoardToolSelect => "Auswählen",
        Key::BoardToolSelectHelp => "Elemente auswählen und verschieben (V)",
        Key::BoardToolRectangle => "Rechteck",
        Key::BoardToolRectangleHelp => "Ein Rechteck zeichnen (R)",
        Key::BoardToolDiamond => "Raute",
        Key::BoardToolDiamondHelp => "Eine Raute zeichnen (D)",
        Key::BoardToolEllipse => "Ellipse",
        Key::BoardToolEllipseHelp => "Eine Ellipse zeichnen (O)",
        Key::BoardToolArrow => "Pfeil",
        Key::BoardToolArrowHelp => "Einen Pfeil zeichnen (A)",
        Key::BoardToolLine => "Linie",
        Key::BoardToolLineHelp => "Eine Linie zeichnen (L)",
        Key::BoardToolFreedraw => "Freihand",
        Key::BoardToolFreedrawHelp => "Freihand zeichnen (P oder X)",
        Key::BoardToolText => "Text",
        Key::BoardToolTextHelp => "Text platzieren, dann tippen (T)",
        Key::BoardLinkTitle => "Notiz verknüpfen",
        Key::BoardLinkHelp => "Ausgewählte Elemente mit einer Notiz verknüpfen (⌘L)",
        Key::BoardLinkName => "Notizname oder -pfad",
        Key::BoardLinkHint => "ein Name oder Tresorpfad · leer hebt die Verknüpfung auf",
        Key::BoardRenameName => "Tafelname",
        Key::BoardPaletteTitle => "Tafelfarben",
        Key::BoardPaletteHelp => "Strich- und Füllfarben",
        Key::BoardPaletteOpen => "Farbpalette",
        Key::BoardPaletteClose => "Farbpalette schließen",
        Key::BoardPaletteStroke => "Strich",
        Key::BoardPaletteFill => "Füllung",
        Key::BoardPaletteSwatchBlack => "schwarz",
        Key::BoardPaletteSwatchGrey => "grau",
        Key::BoardPaletteSwatchRed => "rot",
        Key::BoardPaletteSwatchOrange => "orange",
        Key::BoardPaletteSwatchYellow => "gelb",
        Key::BoardPaletteSwatchGreen => "grün",
        Key::BoardPaletteSwatchBlue => "blau",
        Key::BoardPaletteSwatchPurple => "violett",
        Key::BoardPaletteSwatchTransparent => "transparent",
        Key::GraphPageEmpty => "Keine Notizen zum Darstellen",
        Key::GraphPageFindNode => "Graphknoten suchen",
        Key::GraphPageFindPlaceholder => "Knoten suchen · Enter",
        Key::GraphPageSelected => "AUSGEWÄHLT",
        Key::GraphPageHint => {
            "Pfeiltasten fokussieren · Enter öffnen · +/- zoomen · 0/F einpassen · I isolieren · Ziehen verschieben · Scrollen zoomen"
        }
        Key::GraphScopeGlobal => "Global",
        Key::GraphScopeLocal => "Lokal",
        Key::GraphActionFit => "Einpassen",
        Key::GraphActionReset => "Zurücksetzen",
        Key::GraphActionIsolate => "Isolieren",
        Key::GraphActionOpenNote => "Notiz öffnen",
        Key::KanbanSectionDatesTitle => "Daten und Zeiten",
        Key::KanbanSectionDatesCaption => {
            "Wie Daten und Zeiten eingegeben, geschrieben und angezeigt werden."
        }
        Key::KanbanSectionTagsTitle => "Etiketten",
        Key::KanbanSectionTagsCaption => "Etikett-Chips und die aufbewahrte Etikett-Farbliste.",
        Key::KanbanSectionMetadataTitle => "Metadaten verknüpfter Notizen",
        Key::KanbanSectionMetadataCaption => {
            "Schlüssel am Fuß einer Karte aus ihrer ersten verknüpften Notiz."
        }
        Key::KanbanSectionArchiveTitle => "Archiv",
        Key::KanbanSectionArchiveCaption => {
            "Was das Archivieren schreibt und wie viele Karten das Archiv behält."
        }
        Key::KanbanSectionBoardTitle => "Tafel und Karten",
        Key::KanbanSectionBoardCaption => {
            "Spaltenbreite, Kartenbearbeitung und die Schaltflächen in der Kopfzeile."
        }
        Key::KanbanSectionAppTitle => "App",
        Key::KanbanSectionAppCaption => {
            "Einstellungen für alle Tafeln; es gibt keine tafelweisen Überschreibungen."
        }
        Key::KanbanChoiceEnter => "Enter",
        Key::KanbanChoiceShiftEnter => "Shift+Enter",
        Key::KanbanChoiceStart => "Anfang",
        Key::KanbanChoiceEnd => "Ende",
        Key::KanbanChoicePrepend => "Voranstellen",
        Key::KanbanChoiceAppend => "Anhängen",
        Key::KanbanUiDefaultsNote => {
            "Die eigenen Einstellungen einer Tafel haben Vorrang; andernfalls gelten diese für alle Tafeln."
        }
        Key::KanbanUiBoardSettings => "Tafeleinstellungen",
        Key::KanbanUiChangesWriteOnly => {
            "Änderungen schreiben nur diesen Schlüssel in die Einstellungszeile dieser Notiz und können rückgängig gemacht werden."
        }
        Key::KanbanUiNoMetadataKeys => "Keine Metadatenschlüssel",
        Key::KanbanUiNoTagColours => "Keine Etikettfarben",
        Key::KanbanUiKeptAsWritten => {
            "Unverändert aufbewahrt. Beim Anzeigen wird sie nie neu geschrieben."
        }
        Key::KanbanUiSourceBoardIgnored => "Tafelwert ignoriert",
        Key::KanbanUiSourceBoardOverride => "Tafel-Überschreibung",
        Key::KanbanUiSourceAppIgnored => "App-Wert ignoriert",
        Key::KanbanUiSourceAppDefault => "App-Standard",
        Key::KanbanUiSourceDefault => "Standard",
        Key::KanbanUiResetBuiltIn => "Auf integrierten Wert zurücksetzen",
        Key::KanbanUiResetBoard => "Tafel zurücksetzen",
        Key::KanbanUiResetAppDefault => "App-Standard zurücksetzen",
        Key::KanbanUiRemove => "Entfernen",
        Key::KanbanUiAdd => "Hinzufügen",
        Key::KanbanUiContainsMarkdown => "Enthält Markdown",
        Key::KanbanUiNewMetadataKey => "Neuer Metadatenschlüssel",
        Key::KanbanUiUnlimited => "Unbegrenzt",
        Key::KanbanPageTitle => "Kanban",
        Key::KanbanPageViewSwitcher => "Kanban-Ansicht",
        Key::KanbanPageFilter => "Filtern",
        Key::KanbanPageClearFilters => "Filter zurücksetzen",
        Key::KanbanPageOpenMarkdown => "Als Markdown öffnen",
        Key::KanbanPageResolveConflict => "Konflikt beheben",
        Key::KanbanPageEmptyTitle => "Kein Kanban geöffnet",
        Key::KanbanPageEmptyDetail => {
            if cfg!(target_os = "macos") {
                "Wählen Sie eine Kanban-Notiz und drücken Sie dann ⌘5, um sie als Tafel anzuzeigen."
            } else {
                "Wählen Sie eine Kanban-Notiz und drücken Sie dann Ctrl+5, um sie als Tafel anzuzeigen."
            }
        }
        Key::KanbanPageInvalidTitle => "Diese Notiz kann nicht als Kanban angezeigt werden",
        Key::KanbanPageInvalidDetail => {
            "Ihre Einstellungszeile ist unvollständig oder kein gültiges JSON. Die Datei bleibt unverändert."
        }
        Key::KanbanLaneComplete => "Erledigt",
        Key::KanbanLaneNew => "Neue Spalte",
        Key::KanbanLaneAdd => "Spalte hinzufügen",
        Key::KanbanLaneTitle => "Spaltentitel",
        Key::KanbanLaneCardLimit => "Kartenlimit",
        Key::KanbanLaneNoLimit => "Kein Limit",
        Key::KanbanCardText => "Kartentext",
        Key::KanbanCardOpenLink => "Link öffnen",
        Key::KanbanViewBoard => "Tafel",
        Key::KanbanViewList => "Liste",
        Key::KanbanViewTable => "Tabelle",
        Key::KanbanViewBoardTab => "Tafelansicht",
        Key::KanbanViewListTab => "Listenansicht",
        Key::KanbanViewTableTab => "Tabellenansicht",
        Key::KanbanViewNoLanes => "Keine Spalten auf dieser Tafel",
        Key::KanbanViewNoCards => "Keine Karten",
        Key::KanbanViewNoMatchingCards => "Keine passenden Karten",
        Key::KanbanViewCardsByLane => "Karten nach Spalte",
        Key::KanbanViewColumns => "Spalte, Karte, Datum, Etiketten, Fortschritt",
        Key::KanbanViewColumnLane => "Spalte",
        Key::KanbanViewColumnCard => "Karte",
        Key::KanbanViewColumnDate => "Datum",
        Key::KanbanViewColumnTags => "Etiketten",
        Key::KanbanViewColumnProgress => "Fortschritt",
        Key::KanbanFilterFind => "In der Tafel suchen",
        Key::KanbanFilterTitle => "Karten filtern",
        Key::KanbanFilterText => "Text",
        Key::KanbanFilterDate => "Datum",
        Key::KanbanFilterAnyDate => "Beliebiges Datum",
        Key::KanbanFilterOverdue => "Überfällig",
        Key::KanbanFilterStatus => "Status",
        Key::KanbanFilterAny => "Beliebig",
        Key::KanbanFilterOpen => "Offen",
        Key::KanbanFilterDone => "Erledigt",
        Key::KanbanFilterClearAll => "Alle zurücksetzen",
        Key::KanbanFilterDateError => {
            "Ein Datumsfilter ist YYYY-MM-DD; der Bereich wird nicht angewendet."
        }
        Key::KanbanFilterPreviousMatch => "Vorheriger Treffer",
        Key::KanbanFilterNextMatch => "Nächster Treffer",
        Key::KanbanFilterCloseFind => "Suche schließen",
        Key::KanbanArchiveEmpty => "Keine archivierten Karten.",
        Key::KanbanArchiveDelete => "Löschen",
        Key::KanbanArchiveRestore => "Wiederherstellen",
        Key::KanbanArchiveRestoreTo => "Wiederherstellen nach…",
        Key::KanbanPickerAddDate => "Datum hinzufügen",
        Key::KanbanPickerAddTime => "Uhrzeit hinzufügen",
        Key::KanbanPickerAddADate => "Ein Datum hinzufügen",
        Key::KanbanPickerAddATime => "Eine Uhrzeit hinzufügen",
        Key::KanbanPickerHour => "Stunde",
        Key::KanbanPickerMinute => "Minute",
        Key::KanbanPickerNow => "Jetzt",
        Key::KanbanPickerPreviousMonth => "Vorheriger Monat",
        Key::KanbanPickerNextMonth => "Nächster Monat",
        Key::KanbanPickerDays => "Tage",
        Key::KanbanSettingDateTriggerLabel => "Datumsauslöser",
        Key::KanbanSettingDateTriggerHelp => "Vor einem Datum eingegeben, wie in @{2026-11-12}.",
        Key::KanbanSettingTimeTriggerLabel => "Zeitauslöser",
        Key::KanbanSettingTimeTriggerHelp => "Vor einer Uhrzeit eingegeben, wie in @@{14:30}.",
        Key::KanbanSettingDateFormatLabel => "Datumsformat",
        Key::KanbanSettingDateFormatHelp => {
            "Wird in ein neues Datums-Token geschrieben; YYYY, MM, DD, MMMM und Do sind Tokens."
        }
        Key::KanbanSettingTimeFormatLabel => "Zeitformat",
        Key::KanbanSettingTimeFormatHelp => {
            "Wird in ein neues Zeit-Token geschrieben; HH, mm, h und A sind Tokens."
        }
        Key::KanbanSettingDateDisplayFormatLabel => "Datumsanzeigeformat",
        Key::KanbanSettingDateDisplayFormatHelp => "Wie ein Datum am Kartenfuß angezeigt wird.",
        Key::KanbanSettingShowRelativeDateLabel => "Relatives Datum anzeigen",
        Key::KanbanSettingShowRelativeDateHelp => {
            "Fügt neben dem Datum heute oder in N Tagen hinzu."
        }
        Key::KanbanSettingHideDateInTitleLabel => "Datum im Titel ausblenden",
        Key::KanbanSettingHideDateInTitleHelp => {
            "Hält Datums- und Zeit-Tokens aus dem Haupttext der Karte heraus."
        }
        Key::KanbanSettingLinkDateToDailyNoteLabel => "Datum mit Tagesnotiz verknüpfen",
        Key::KanbanSettingLinkDateToDailyNoteHelp => {
            "Macht das angezeigte Datum zu einem Link zu seiner Tagesnotiz."
        }
        Key::KanbanSettingHideTagsInTitleLabel => "Etiketten im Titel ausblenden",
        Key::KanbanSettingHideTagsInTitleHelp => {
            "Hält #tag-Tokens aus dem Haupttext der Karte heraus."
        }
        Key::KanbanSettingShowTagsAtBottomLabel => "Etiketten unten anzeigen",
        Key::KanbanSettingShowTagsAtBottomHelp => "Zeichnet Etikett-Chips am Kartenfuß.",
        Key::KanbanSettingTagColorsLabel => "Etikettfarben",
        Key::KanbanSettingTagColorsHelp => {
            "Unverändert aufbewahrt. Muninn hat noch keinen Farbeditor."
        }
        Key::KanbanSettingLinkedPageMetadataLabel => "Metadaten verknüpfter Notizen",
        Key::KanbanSettingLinkedPageMetadataHelp => {
            "Schlüssel, die am Kartenfuß aus ihrer ersten verknüpften Notiz angezeigt werden."
        }
        Key::KanbanSettingShowMissingMetadataLabel => "Fehlende Metadaten anzeigen",
        Key::KanbanSettingShowMissingMetadataHelp => {
            "Zeichnet Schlüssel: —, wenn die verknüpfte Notiz oder der Schlüssel fehlt."
        }
        Key::KanbanSettingArchiveWithDateLabel => "Mit Datum archivieren",
        Key::KanbanSettingArchiveWithDateHelp => {
            "Fügt beim Archivieren das Datum zum Kartentext hinzu."
        }
        Key::KanbanSettingArchiveDateFormatLabel => "Archiv-Datumsformat",
        Key::KanbanSettingArchiveDateFormatHelp => {
            "Das beim Archivieren verwendete Datumsformat; dieselben Tokens wie Datumsformat."
        }
        Key::KanbanSettingArchiveDatePositionLabel => "Position des Archivdatums",
        Key::KanbanSettingArchiveDatePositionHelp => {
            "Anfang oder Ende des Texts der archivierten Karte."
        }
        Key::KanbanSettingArchiveDateSeparatorLabel => "Trennzeichen des Archivdatums",
        Key::KanbanSettingArchiveDateSeparatorHelp => {
            "Text zwischen der Karte und ihrem Datum; ein leerer Wert verbindet beide."
        }
        Key::KanbanSettingMaxArchiveCardsLabel => "Max. Archivkarten",
        Key::KanbanSettingMaxArchiveCardsHelp => {
            "Ältere Archivkarten über diese Anzahl hinaus werden entfernt. 0 ist unbegrenzt."
        }
        Key::KanbanSettingLaneWidthLabel => "Spaltenbreite",
        Key::KanbanSettingLaneWidthHelp => {
            "Spaltenbreite in Punkten; die Tafel begrenzt die Darstellung auf 120–720."
        }
        Key::KanbanSettingNewLineTriggerLabel => "Zeilenumbruch-Auslöser",
        Key::KanbanSettingNewLineTriggerHelp => {
            "Im Karteneditor schließt Enter ab und Shift+Enter bricht die Zeile, oder umgekehrt."
        }
        Key::KanbanSettingNewCardInsertionLabel => "Position neuer Karten",
        Key::KanbanSettingNewCardInsertionHelp => {
            "Wohin „+ Karte hinzufügen“ die neue Karte setzt."
        }
        Key::KanbanSettingShowCheckboxesLabel => "Kontrollkästchen anzeigen",
        Key::KanbanSettingShowCheckboxesHelp => {
            "Zeichnet das Erledigt-Kontrollkästchen jeder Karte."
        }
        Key::KanbanSettingShowFilterButtonLabel => "Filter-Schaltfläche",
        Key::KanbanSettingShowFilterButtonHelp => "Zeigt die Filter-Schaltfläche in der Kopfzeile.",
        Key::KanbanSettingShowArchiveButtonLabel => "Archiv-Schaltfläche",
        Key::KanbanSettingShowArchiveButtonHelp => {
            "Zeigt die Archiv-Schaltfläche in der Kopfzeile."
        }
        Key::KanbanSettingShowOpenMarkdownButtonLabel => "Schaltfläche „Als Markdown öffnen“",
        Key::KanbanSettingShowOpenMarkdownButtonHelp => {
            "Zeigt die Schaltfläche „Als Markdown öffnen“ in der Kopfzeile."
        }
        Key::KanbanSettingNewNoteFolderLabel => "Ordner für neue Notizen",
        Key::KanbanSettingNewNoteFolderHelp => {
            "Ordner für „Neue Notiz aus Karte“; leer gilt der Ordner, den „Neue Notiz“ verwendet."
        }
        Key::KanbanSettingNewNoteTemplateLabel => "Vorlage für neue Notizen",
        Key::KanbanSettingNewNoteTemplateHelp => {
            "Ein Notizpfad, dessen Text am Anfang der neuen Notiz steht; leer verwendet die App-Notizvorlage."
        }
        Key::KanbanSettingKanbanAutosaveLabel => "Kanban-Änderungen automatisch speichern",
        Key::KanbanSettingKanbanAutosaveHelp => {
            "Speichert ungespeicherte Kanban-Änderungen, sobald sie ruhen. Aus überlässt das Speichern ⌘S."
        }
        Key::StatusSaving => "Wird gespeichert…",
        Key::StatusSaved => "Gespeichert",
        Key::StatusAutosaveOn => "Automatisches Speichern ein",
        Key::StatusRetry => "Erneut versuchen",
        Key::StatusUndo => "Rückgängig",
        Key::RecoveryRestore => "Wiederherstellen",
        Key::RecoveryRestoreAll => "Alle wiederherstellen",
        Key::RecoveryDismiss => "Verwerfen",
        Key::RecoveryWriteEdits => "— Textänderungen",
        Key::RecoveryKanbanEdits => "— Kanban-Änderungen",
        Key::BrokenAllResolve => "Alle Links funktionieren",
        Key::BrokenTitle => "Fehlerhafte Links",
        Key::TopbarNoNoteSelected => "Keine Notiz ausgewählt",
        Key::TopbarOpenAsKanban => "Als Kanban öffnen",
        Key::TrashTitle => "Papierkorb",
        Key::TrashEmptyAction => "Papierkorb leeren",
        Key::TrashExplainer => {
            "Gelöschte Notizen bleiben hier bis zum Rückgängigmachen oder endgültigen Entfernen. Höchstens 500 Dateien oder 100 MB werden aufbewahrt; der älteste Zeitstempelordner wird zuerst entfernt."
        }
        Key::TrayShow => "Muninn anzeigen",
        Key::TrayNewNote => "Neue Notiz",
        Key::TraySyncNow => "Jetzt synchronisieren",
        Key::TrayQuit => "Muninn beenden",
        Key::TrayNotConnected => "Nicht verbunden.",
        Key::TrayTooltip => "Muninn",
        Key::ErrorEnterVaultPath => "Geben Sie einen Tresorordnerpfad ein",
        Key::ErrorEnterNewVaultPath => "Geben Sie einen Ordnerpfad für den neuen Tresor ein",
        Key::ErrorSaveNoteBeforeSwitch => {
            "Speichern oder klären Sie die aktuelle Notiz, bevor Sie den Tresor wechseln"
        }
        Key::ErrorSaveBoardBeforeSwitch => {
            "Speichern Sie die aktuelle Tafel, bevor Sie den Tresor wechseln"
        }
        Key::ErrorSaveKanbanBeforeSwitch => {
            "Speichern Sie das aktuelle Kanban, bevor Sie den Tresor wechseln"
        }
        Key::ErrorSaveNoteBeforeRestore => {
            "Speichern oder verwerfen Sie die aktuelle Notiz, bevor Sie eine andere wiederherstellen"
        }
        Key::ErrorSaveBoardBeforeRestore => {
            "Speichern oder verwerfen Sie die aktuelle Tafel, bevor Sie eine andere wiederherstellen"
        }
        Key::ErrorSaveBoardBeforeRescan => {
            "Speichern Sie die aktuelle Tafel, bevor Sie den Tresor erneut scannen"
        }
        Key::ErrorOpenerHintUnix => {
            "Installieren Sie xdg-utils (für xdg-open) oder öffnen Sie die Datei manuell."
        }
        Key::ErrorNoFileOpener => {
            "Für diese Plattform ist kein Dateiöffner dokumentiert; öffnen Sie die Datei manuell"
        }
        Key::ErrorOpenerHintMacos => {
            "Der Systembefehl `open` ist fehlgeschlagen; öffnen Sie die Datei im Finder."
        }
        Key::ErrorOpenerHintWindows => {
            "Der Systemöffner ist fehlgeschlagen; öffnen Sie die Datei im Explorer."
        }
        Key::ErrorOpenerHintOther => "Öffnen Sie die Datei manuell.",
        Key::ErrorDocumentBoard => "Tafel",
        Key::ErrorDocumentKanban => "Kanban",
        Key::ImportDocument => "Dokument importieren…",
        Key::ImportDocumentHelp => "Ein Dokument als neue Notiz importieren",
        Key::ImportDialogTitle => "Dokument importieren",
        Key::ImportDialogFilterDocuments => "Dokumente",
        Key::ErrorImportDialogUnavailable => {
            "Kein Dateidialog auf diesem Desktop — ziehen Sie das Dokument stattdessen auf das Fenster."
        }
        Key::StatusTrashEmptied => "Papierkorb geleert",
        Key::StatusLinkCopied => "Link zur Karte kopiert",
        Key::ConflictExternalChange => "Externe Änderung erkannt",
        Key::ConflictFileDeleted => "Datei extern gelöscht",
        Key::ConflictChangedOnDisk => "Von einer anderen App auf dem Datenträger geändert",
        Key::ConflictMissingOnDisk => "Auf dem Datenträger entfernt oder umbenannt",
        Key::ConflictKeepMine => "Meine Version behalten",
        Key::ConflictLoadTheirs => "Andere Version laden",
        Key::ConflictSaveCopy => "Kopie speichern",
        Key::ConflictDiscard => "Verwerfen",
        Key::CommonAppName => "Muninn",
        Key::CommonDelete => "Löschen",
        Key::CommonEdit => "Bearbeiten",
        Key::CommonMove => "Verschieben",
        Key::CommonNewNote => "Neue Notiz",
        Key::CommonOpen => "Öffnen",
        Key::CommonRename => "Umbenennen",
        Key::CommonResults => "Ergebnisse",
        Key::CommonVaultRoot => "Stammverzeichnis",
        Key::CommonDismiss => "Verwerfen",
        Key::CommonOn => "Ein",
        Key::CommonOff => "Aus",
        Key::SidebarNoNotesTitle => "Noch keine Notizen",
        Key::SidebarNoNotesHint => "Legen Sie .md-Dateien hier ab oder erstellen Sie unten eine.",
        Key::SidebarNoMatchTitle => "Keine passenden Notizen",
        Key::SidebarNoMatchHint => "Versuchen Sie eine andere Suche.",
        Key::SidebarSearch => "Notizen durchsuchen",
        Key::SidebarLinksOk => "Links ok",
        Key::SidebarKanbans => "KANBANS",
        Key::SidebarBoards => "TAFELN",
        Key::SidebarTheVaultRoot => "das Stammverzeichnis",
        Key::SettingsTitle => "Einstellungen",
        Key::TopbarSettingsHelp => "Einstellungen (Ctrl+,)",
        Key::TopbarHideSidebar => "Seitenleiste ausblenden",
        Key::TopbarShowSidebar => "Seitenleiste anzeigen",
        Key::SettingsChangesSave => "Änderungen werden automatisch gespeichert.",
        Key::SettingsClose => "Einstellungen schließen (Esc)",
        Key::SettingsSection => "Einstellungsbereich",
        Key::SettingsAutosave => "Automatisches Speichern",
        Key::SettingsDiagnostics => "Diagnose",
        Key::SettingsCopyDiagnostics => "Diagnose kopieren",
        Key::SettingsReportIssue => "Ein Problem auf GitHub melden",
        Key::SettingsThemeOmarchy => "Omarchy",
        Key::SettingsThemeSystem => "System",
        Key::SettingsThemeDark => "Dunkel",
        Key::SettingsThemeLight => "Hell",
        Key::SettingsMotionSystem => "System",
        Key::SettingsMotionOn => "Ein",
        Key::SettingsMotionOff => "Aus",
        Key::SettingsEffectsClassic => "Klassisch",
        Key::SettingsEffectsElectric => "Elektrisch",
        Key::SettingsEffectsOff => "Aus / Minimal",
        Key::SettingsIntensitySubtle => "Dezent",
        Key::SettingsIntensityBalanced => "Ausgewogen",
        Key::SettingsIntensityIntense => "Intensiv",
        Key::WelcomeGraphLabel => "Den Link-Graphen öffnen",
        Key::WelcomeGraphHelp => "Sehen Sie, wie Notizen über [[Links]] verbunden sind",
        Key::WelcomeGraphPill => "[[eine Notiz]] → Graph",
        Key::PreviewEmptyNote => "Diese Notiz ist leer. Schreiben Sie etwas im Editor.",
        Key::PreviewStartWriting => "Erstellen Sie eine Notiz, um mit dem Schreiben zu beginnen.",
        Key::PreviewEditor => "Notizeditor",
        Key::PreviewEditorPlaceholder => {
            "Beginnen Sie zu schreiben. Tippen Sie [[, um eine andere Notiz zu verlinken."
        }
        Key::PreviewCompletions => "Vervollständigungen",
        Key::PreviewAttachments => "ANHÄNGE",
        Key::PreviewAttachmentsNote => "Dateien bleiben außerhalb des Notizindex",
        Key::PreviewOpen => "Öffnen",
        Key::PreviewOpenBoard => "Tafel öffnen",
        Key::PreviewMissingFile => "fehlende Datei",
        Key::PreviewBoardKind => "Tafel",
        Key::PreviewAttachmentKind => "Anhang",
        Key::PreviewMissing => "fehlt",
        Key::PreviewNoOutgoing => "Keine ausgehenden Links",
        Key::PreviewNoBacklinks => "Noch keine Backlinks",
        Key::PreviewLinks => "LINKS",
        Key::PreviewOutgoing => "AUSGEHEND",
        Key::PreviewBacklinks => "BACKLINKS",
        Key::PreviewGraph => "GRAPH",
        Key::PreviewGraphHint => "Diese Notiz und alles, was sie berührt, ansehen.",
        Key::PreviewFind => "In der Notiz suchen",
        Key::StartupScanning => "Ihr Tresor wird gescannt",
        Key::StartupScanCancelled => "Scan abgebrochen",
        Key::StartupCouldNotOpen => "Tresor konnte nicht geöffnet werden",
        Key::StartupRetryHint => {
            "„Erneut versuchen“ scannt den Tresor erneut, oder wählen Sie in den Einstellungen (Ctrl+,) einen anderen Ordner."
        }
        Key::StartupPickFolderHint => {
            "Wählen Sie in den Einstellungen (Ctrl+,) einen anderen Ordner oder übergeben Sie --vault / setzen Sie MUNINN_VAULT."
        }
        Key::ModeWrite => "Schreiben",
        Key::ModeRead => "Lesen",
        Key::ModeGraph => "Graph",
        Key::ModeBoard => "Tafel",
        Key::ModeKanban => "Kanban",
        Key::FocusExit => "Fokusmodus verlassen",
        Key::FocusEnter => "Fokusmodus",
        Key::MenuOpenExternally => "Extern öffnen",
        Key::MenuReveal => "Im Dateimanager anzeigen",
        Key::MenuMoveTo => "Verschieben nach…",
        Key::MenuNewNoteHere => "Neue Notiz hier",
        Key::MenuNewFolder => "Neuer Ordner…",
        Key::MenuInsertBefore => "Karte davor einfügen",
        Key::MenuInsertAfter => "Karte danach einfügen",
        Key::MenuDuplicate => "Duplizieren",
        Key::MenuMarkOpen => "Als offen markieren",
        Key::MenuMarkDone => "Als erledigt markieren",
        Key::MenuChangeDate => "Datum ändern…",
        Key::MenuAddDate => "Datum hinzufügen…",
        Key::MenuChangeTime => "Uhrzeit ändern…",
        Key::MenuAddTime => "Uhrzeit hinzufügen…",
        Key::MenuSplitCard => "Karte teilen",
        Key::MenuCopyLink => "Link zur Karte kopieren",
        Key::MenuNewNoteFromCard => "Neue Notiz aus Karte",
        Key::MenuMarkAllDone => "Alle Karten als erledigt markieren",
        Key::MenuNoteActions => "Notizaktionen",
        Key::MenuFolderActions => "Ordneraktionen",
        Key::MenuBoardActions => "Tafelaktionen",
        Key::MenuCardActions => "Kartenaktionen",
        Key::MenuLaneActions => "Spaltenaktionen",
        Key::MenuArchiveActions => "Archivaktionen",
        Key::MenuMoveToLane => "In eine Spalte verschieben",
        Key::LinkImpactHere => "hier",
        Key::LinkImpactIntoFolder => "in diesen Ordner",
        Key::LinkThisNote => "Diese Notiz",
        Key::LinkNameFallback => "Link",
        Key::RenameRewriteLinks => "Links auf den neuen Namen umschreiben",
        Key::RenameLeaveDangling => "Links ins Leere laufen lassen",
        Key::DeleteNoteQuestion => "Diese Notiz löschen?",
        Key::DeleteFolderQuestion => "Ordner löschen? (nur leer)",
        Key::NewNoteName => "Name der neuen Notiz",
        Key::NoteNamePlaceholder => "Notizname",
        Key::RenameNoteName => "Notiz umbenennen",
        Key::FolderNamePlaceholder => "Ordnername",
        Key::NewFolderName => "Name des neuen Ordners",
        Key::RenameFolderName => "Ordner umbenennen",
        Key::PaletteMissingNote => "Fehlende Notiz",
        Key::PaletteKanbanRoot => "Kanban · Stammverzeichnis",
        Key::BoardRenameSaveFirst => {
            "Speichern Sie die Tafel zuerst: sie konnte nicht gespeichert werden"
        }
        Key::KanbanFilterToday => "heute",
        Key::KanbanFilterOverdueWord => "überfällig",
        Key::KanbanFilterOpenWord => "offen",
        Key::KanbanFilterDoneWord => "erledigt",
        Key::KanbanDailyNoteLink => "Die Tagesnotiz des Datums öffnen",
        Key::KanbanLaneMoveUp => "Nach oben verschieben",
        Key::KanbanLaneMoveTop => "An den Anfang verschieben",
        Key::KanbanLaneMoveDown => "Nach unten verschieben",
        Key::KanbanLaneMoveBottom => "An das Ende verschieben",
        Key::KanbanLaneMovePreviousList => "In die vorherige Liste verschieben",
        Key::KanbanLaneMoveNextList => "In die nächste Liste verschieben",
        Key::KanbanLaneMoveToList => "In eine Liste verschieben…",
        Key::KanbanLaneInsertBefore => "Spalte davor einfügen",
        Key::KanbanLaneInsertAfter => "Spalte danach einfügen",
        Key::KanbanLaneMoveLeft => "Spalte nach links verschieben",
        Key::KanbanLaneMoveRight => "Spalte nach rechts verschieben",
        Key::KanbanLaneRename => "Spalte umbenennen",
        Key::KanbanLaneLimit => "Kartenlimit…",
        Key::KanbanLaneMarkNotDone => "Als nicht erledigt markieren",
        Key::KanbanLaneMarkDone => "Als erledigt markieren",
        Key::KanbanLaneSortText => "Nach Text sortieren",
        Key::KanbanLaneSortDate => "Nach Datum sortieren",
        Key::KanbanLaneSortTags => "Nach Etiketten sortieren",
        Key::KanbanLaneDelete => "Spalte löschen",
        Key::KanbanLaneDeleteCards => "Die Spalte und ihre Karten löschen",
        Key::KanbanLaneDeleteText => "Die Spalte und ihren Notiztext löschen",
        Key::KanbanCardFallback => "Karte",
        Key::KanbanCardMarkDone => "Als erledigt markieren",
        Key::KanbanCardMarkOpen => "Als nicht erledigt markieren",
        Key::KanbanCardChecklistItem => "Checklistenpunkt",
        Key::KanbanEditAddCard => "die Karte hinzufügen",
        Key::KanbanEditEditCard => "die Karte bearbeiten",
        Key::KanbanEditDeleteCard => "die Karte löschen",
        Key::KanbanEditDuplicateCard => "die Karte duplizieren",
        Key::KanbanEditSplitCard => "die Karte teilen",
        Key::KanbanEditChangeDoneMark => "die Erledigt-Markierung ändern",
        Key::KanbanEditToggleChecklistItem => "den Checklistenpunkt umschalten",
        Key::KanbanEditMarkCardsDone => "die Karten als erledigt markieren",
        Key::KanbanEditAddLane => "die Spalte hinzufügen",
        Key::KanbanEditRenameLane => "die Spalte umbenennen",
        Key::KanbanEditClearLimit => "das Limit entfernen",
        Key::KanbanEditSetLimit => "das Limit setzen",
        Key::KanbanEditDeleteLane => "die Spalte löschen",
        Key::KanbanEditChangeDoneLane => "die Erledigt-Spalte ändern",
        Key::KanbanEditSortLane => "die Spalte sortieren",
        Key::KanbanEditCollapseLane => "die Spalte einklappen",
        Key::KanbanEditMoveCard => "die Karte verschieben",
        Key::KanbanEditChangeDate => "das Datum ändern",
        Key::KanbanEditChangeTime => "die Uhrzeit ändern",
        Key::KanbanEditClearDate => "das Datum entfernen",
        Key::KanbanEditClearTime => "die Uhrzeit entfernen",
        Key::KanbanEditChangeBoardSetting => "die Tafeleinstellung ändern",
        Key::KanbanEditResetBoardSetting => "die Tafeleinstellung zurücksetzen",
        Key::KanbanEditCopyCardLink => "den Kartenlink kopieren",
        Key::KanbanEditMakeNoteFromCard => "eine Notiz aus der Karte erstellen",
        Key::KanbanEditArchiveCard => "die Karte archivieren",
        Key::KanbanEditRestoreCard => "die archivierte Karte wiederherstellen",
        Key::KanbanEditDeleteArchivedCard => "die archivierte Karte löschen",
        Key::KanbanStatusLaneRefused => "die bearbeitete Spalte wurde abgelehnt",
        Key::KanbanStatusCardRefused => "die bearbeitete Karte wurde abgelehnt",
        Key::KanbanStatusNotInsideVault => "das Kanban liegt nicht im geöffneten Tresor",
        Key::KanbanStatusNoteGone => "die Notiz ist nicht mehr im Tresor",
        Key::KanbanStatusFileUnchecked => "die Datei konnte nicht geprüft werden",
        Key::KanbanStatusFileRemoved => "die Datei wurde entfernt",
        Key::KanbanStatusFileChanged => "die Datei wurde auf dem Datenträger geändert",
        Key::KanbanTimeInvalid => {
            "Eine Uhrzeit braucht eine Stunde von 0 bis 23 und eine Minute von 0 bis 59"
        }
        Key::KanbanLaneLimitInvalid => "Ein Kartenlimit ist eine ganze Zahl",
        Key::VaultEmptyFolderName => "leerer Ordnername",
        Key::VaultCannotRenameRoot => "das Stammverzeichnis kann nicht umbenannt werden",
        Key::VaultCannotDeleteRoot => "das Stammverzeichnis kann nicht gelöscht werden",
        Key::VaultDestinationExists => "Ziel existiert bereits",
        Key::BoardParseNotObject => "eine Tafel muss ein JSON-Objekt sein",
        Key::BoardParseNotExcalidraw => "keine Excalidraw-Szene (type ist nicht \"excalidraw\")",
        Key::BoardParseVersionNumber => "die Szenenversion muss eine ganze Zahl sein",
        Key::BoardParseSourceString => "die Szenenquelle muss eine Zeichenkette sein",
        Key::BoardParseElementsArray => "die Szenenelemente müssen ein Array sein",
        Key::BoardStatusUnreadable => {
            "die Tafel konnte nicht gelesen werden und wird daher nicht geschrieben"
        }
        Key::BoardStatusOtherVault => "die Tafel gehört zu einem anderen Tresor",
        Key::BoardStatusInsideVault => "die Tafel liegt nicht im geöffneten Tresor",
        Key::KanbanEditMoveLane => "die Spalte verschieben",
        Key::AutostartPathNotUtf8 => "der Pfad der ausführbaren Datei ist nicht UTF-8",
        Key::AutostartPathLineBreak => {
            "der Pfad der ausführbaren Datei enthält einen Zeilenumbruch"
        }
        Key::NoteNoLongerIndexed => "Notiz ist nicht mehr indexiert",
        Key::TrashEmpty => "Der Papierkorb ist leer",
        Key::CoreBoardNameEmpty => "darf nicht leer sein",
        Key::CoreBoardNameSeparators => "darf weder / noch \\ enthalten",
        Key::CoreBoardNameHidden => "darf nicht mit einem Punkt beginnen",
        Key::CoreBoardNameForbidden => "darf weder : * ? \" < > | noch Steuerzeichen enthalten",
        Key::CoreBoardNameTrailingDot => "darf nicht mit einem Punkt enden",
        Key::CoreBoardNameReserved => "darf kein reservierter Gerätename sein",
    }
}
