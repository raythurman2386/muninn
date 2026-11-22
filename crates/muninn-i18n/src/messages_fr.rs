//! The `fr-FR` template table for [`MsgKey`].
//!
//! Mirrors [`crate::messages`] arm for arm and in the same order. Every
//! placeholder keeps its position in a natural French sentence; the
//! one/other pairs are separate rows, exactly as in English, so the CLDR
//! French rule decides the branch: 0 and 1 pick the singular row
//! (`0 note`, `1 note`) and 2 picks the plural (`2 notes`).
//!
//! French typography applies to these sentences too: a no-break space U+00A0
//! before `:` `;` `!` `?` and the typographic apostrophe U+2019 for
//! elisions. The space is written as a Rust escape where it would otherwise
//! be invisible.
//!
//! The copy is machine-authored and awaits native/fluent review before
//! release.

use crate::msg::MsgKey;

/// The French template for `key`.
///
/// Private: callers go through [`crate::phrase`] or [`crate::phrase_current`],
/// which substitute the arguments and pick the language. Keeping every table
/// behind those functions is what makes a second language a change to this
/// crate rather than to call sites.
pub(crate) fn template(key: MsgKey) -> &'static str {
    match key {
        MsgKey::FilesSkippedOne => "{0} fichier ignoré",
        MsgKey::FilesSkippedOther => "{0} fichiers ignorés",
        MsgKey::NoteOne => "{0} note",
        MsgKey::NoteOther => "{0} notes",
        MsgKey::BoardOne => "{0} tableau",
        MsgKey::BoardOther => "{0} tableaux",
        MsgKey::TargetOne => "{0} cible",
        MsgKey::TargetOther => "{0} cibles",
        MsgKey::ReferrerOne => "{0} référent",
        MsgKey::ReferrerOther => "{0} référents",
        MsgKey::WordOne => "{0} mot",
        MsgKey::WordOther => "{0} mots",
        MsgKey::LinkOne => "{0} lien",
        MsgKey::LinkOther => "{0} liens",
        MsgKey::BacklinkOne => "{0} rétrolien",
        MsgKey::BacklinkOther => "{0} rétroliens",
        MsgKey::GraphOutgoingLinkOne => "{0} lien sortant",
        MsgKey::GraphOutgoingLinkOther => "{0} liens sortants",
        MsgKey::ResultOne => "{0} résultat",
        MsgKey::ResultOther => "{0} résultats",
        MsgKey::ElementOne => "{0} élément",
        MsgKey::ElementOther => "{0} éléments",
        MsgKey::ArchivedCardOne => "{0} carte archivée",
        MsgKey::ArchivedCardOther => "{0} cartes archivées",
        MsgKey::LineOne => "{0} ligne",
        MsgKey::LineOther => "{0} lignes",
        MsgKey::FileOne => "{0} fichier",
        MsgKey::FileOther => "{0} fichiers",

        MsgKey::StatusCounts => "{0} · {1} · {2}",
        MsgKey::TrashSummary => "Corbeille · {0} · {1}",
        MsgKey::NoteLinks => "{0} · {1}",
        MsgKey::NoLinks => "aucun lien",
        MsgKey::NoteLinksRoot => "racine du coffre · {0}",
        MsgKey::NoteLinksFolder => "{0} · {1}",
        MsgKey::SidebarSummary => "{0} · {1}",
        MsgKey::GraphLinks => "{0} · {1}",
        MsgKey::RecoveredCopies => "{0} et {1}",

        MsgKey::MoreEdits => "+{0} de plus",
        MsgKey::Deleted => "Supprimé\u{a0}: {0}",
        MsgKey::ScanMs => "analyse {0} ms",
        MsgKey::RestoreTo => "Restaurer dans {0}",
        MsgKey::FilterByTag => "Filtrer par #{0}",
        MsgKey::LinkImpactOne => "{0} note renvoie à {1}",
        MsgKey::LinkImpactOther => "{0} notes renvoient à {1}",

        MsgKey::CouldNotSave => "Impossible d’enregistrer\u{a0}: {0}",
        MsgKey::CouldNotOpenVault => "Impossible d’ouvrir le coffre\u{a0}: {0}",
        MsgKey::CouldNotReadVault => "Impossible de lire le coffre\u{a0}: {0}",
        MsgKey::CouldNotReadNoteState => "Impossible de lire l’état de la note\u{a0}: {0}",
        MsgKey::CouldNotCreateVault => "Impossible de créer le coffre\u{a0}: {0}",
        MsgKey::CouldNotPersistKanbanRecovery => {
            "Impossible d’enregistrer la copie de récupération du kanban\u{a0}: {0}"
        }
        MsgKey::CouldNotPersistRecovery => {
            "Impossible d’enregistrer la copie de récupération\u{a0}: {0}"
        }
        MsgKey::CouldNotPersistBoardRecovery => {
            "Impossible d’enregistrer la copie de récupération du tableau\u{a0}: {0}"
        }
        MsgKey::CouldNotReadRecovery => "Impossible de lire la copie de récupération\u{a0}: {0}",
        MsgKey::CouldNotRecreateNoteFolder => {
            "Impossible de recréer le dossier des notes\u{a0}: {0}"
        }
        MsgKey::CouldNotRecreateNote => "Impossible de recréer la note\u{a0}: {0}",
        MsgKey::CouldNotReloadNote => "Impossible de recharger la note\u{a0}: {0}",
        MsgKey::CouldNotSaveCopy => "Impossible d’enregistrer une copie\u{a0}: {0}",
        MsgKey::CouldNotRenameNote => "Impossible de renommer la note\u{a0}: {0}",
        MsgKey::CouldNotRenameFolder => "Impossible de renommer le dossier\u{a0}: {0}",
        MsgKey::CouldNotDeleteNote => "Impossible de supprimer la note\u{a0}: {0}",
        MsgKey::CouldNotDeleteFolder => "Impossible de supprimer le dossier\u{a0}: {0}",
        MsgKey::CouldNotEmptyTrash => "Impossible de vider la corbeille\u{a0}: {0}",
        MsgKey::CouldNotMoveNote => "Impossible de déplacer la note\u{a0}: {0}",
        MsgKey::CouldNotCreateFolder => "Impossible de créer le dossier\u{a0}: {0}",
        MsgKey::CouldNotCreateNote => "Impossible de créer une note\u{a0}: {0}",
        MsgKey::CouldNotAddNoteTemplate => "Impossible d’ajouter le modèle de note\u{a0}: {0}",
        MsgKey::CouldNotBeRead => "impossible de lire le {0}",
        MsgKey::CouldNotRestore => "Impossible de restaurer {0}\u{a0}: {1}",
        MsgKey::CouldNotOpenFile => "Impossible d’ouvrir {0}\u{a0}: {1}. {2}",
        MsgKey::CouldNotRevealFile => "Impossible d’afficher {0}\u{a0}: {1}. {2}",
        MsgKey::CouldNotImportInto => "Impossible d’importer dans {0}\u{a0}: {1}",
        MsgKey::RefusingUnsafeRecoveryPath => "Chemin de récupération non sûr refusé\u{a0}: {0}",
        MsgKey::BoardQuitRefused => {
            "Tableau non enregistré et aucune copie de récupération n’a pu être écrite\u{a0}: {0}. Utilisez Enregistrer une copie ou corrigez le dossier, puis quittez à nouveau."
        }
        MsgKey::ScanCancelled => "Analyse annulée après {0} notes sur {1}",

        MsgKey::ImportRefused => "Importation refusée\u{a0}: {0}",
        MsgKey::ImportPartlyRefused => {
            "Importation partielle\u{a0}: {0} sur {1}\u{a0}; refusés\u{a0}: {2}"
        }
        MsgKey::CannotImport => {
            "Impossible d’importer {0}\u{a0}: déposez des documents Markdown, image, PDF ou bureautique"
        }
        MsgKey::ImportProgress => "Importation de {0}…",
        MsgKey::ImportDocumentNotice => "{0} importé",
        MsgKey::ImportUnsupported => {
            "Impossible d’importer {0}\u{a0}: ce type de fichier ne peut pas être converti."
        }
        MsgKey::ImportNeedsOcrOne => {
            "Impossible d’importer {0}\u{a0}: la page {1} sur {2} est une image numérisée et nécessite l’OCR, que Muninn ne fait pas. Exportez d’abord une version texte."
        }
        MsgKey::ImportNeedsOcrOther => {
            "Impossible d’importer {0}\u{a0}: les pages {1} sur {2} sont des images numérisées et nécessitent l’OCR, que Muninn ne fait pas. Exportez d’abord une version texte."
        }
        MsgKey::ImportEncrypted => {
            "Impossible d’importer {0}\u{a0}: le document est chiffré. Supprimez son mot de passe et importez-le à nouveau."
        }
        MsgKey::ImportMalformed => {
            "Impossible d’importer {0}\u{a0}: le fichier est endommagé ou n’est pas un document lisible."
        }
        MsgKey::ImportResourceLimit => {
            "Impossible d’importer {0}\u{a0}: le document est trop complexe à convertir."
        }
        MsgKey::ImportMissingPart => {
            "Impossible d’importer {0}\u{a0}: il manque au document une partie nécessaire."
        }
        MsgKey::ImportIo => "Impossible d’importer {0}\u{a0}: le fichier n’a pas pu être lu.",
        MsgKey::ImportFailed => {
            "Impossible d’importer {0}\u{a0}: le document n’a pas pu être converti."
        }
        MsgKey::ImportTooLarge => {
            "Impossible d’importer {0}\u{a0}: le fichier fait {1}, au-delà de la limite de {2}."
        }
        MsgKey::ImportBodyTooLarge => {
            "Impossible d’importer {0}\u{a0}: la note convertie fait {1}, au-delà de la limite de {2}."
        }
        MsgKey::RefusedNotAFile => "{0}\u{a0}: n’est pas un fichier",
        MsgKey::RefusedUnsupportedType => "{0}\u{a0}: type de fichier non pris en charge",
        MsgKey::RefusedAlreadyExists => "{0}\u{a0}: existe déjà dans ce dossier",

        MsgKey::ConnectedAs => {
            "Connecté en tant que {0}. Le jeton est dans le magasin d’identifiants du système. Lancez Synchroniser maintenant quand vous êtes prêt."
        }
        MsgKey::LastSync => "Dernière synchronisation réussie\u{a0}: {0}.",
        MsgKey::SyncStatus => "Synchronisation\u{a0}: {0}",
        MsgKey::SyncExcluded => "Chemins exclus par le contrat de synchronisation\u{a0}: {0}.",
        MsgKey::SyncConflicts => "{0} conflit(s)\u{a0}: {1}. Rien n’a été écrasé.",
        MsgKey::SyncConflictCopies => {
            "Une note a changé sur deux appareils\u{a0}; les deux versions ont été conservées\u{a0}: {0}."
        }

        MsgKey::BoardNameTaken => "Un tableau nommé «\u{a0}{0}\u{a0}» existe déjà dans ce dossier",
        MsgKey::NoNoteNamed => "Aucune note nommée «\u{a0}{0}\u{a0}» dans ce coffre",
        MsgKey::LinkedNoteHint => "Note liée\u{a0}: {0} · ⌘Enter ouvre",
        MsgKey::BoardCanvas => "Canevas du tableau, {0}, outil {1}",
        MsgKey::BoardCanvasSelected => "Canevas du tableau, {0}, outil {1}, {2} sélectionné(s)",
        MsgKey::CardLabelEmpty => "{0}, carte vide",
        MsgKey::CardLabel => "{0}\u{a0}: {1}",

        MsgKey::ThemeLabel => "Thème\u{a0}: {0} — cliquez pour changer (Système, Sombre, Clair)",
        MsgKey::AutosaveHelp => "La sauvegarde automatique écrit {0} s après l’arrêt de la saisie.",
        MsgKey::Version => "Version {0}.",
        MsgKey::UpdateAvailable => "Muninn {0} est disponible",
        MsgKey::ShowWithChord => "Afficher Muninn avec {0}",
        MsgKey::ShortcutFailed => "Impossible d’enregistrer {0}\u{a0}: {1}",
        MsgKey::ShortcutOn => "Activé. {0} affiche Muninn depuis n’importe quelle application.",

        MsgKey::RelativeToday => "aujourd’hui",
        MsgKey::RelativeTomorrow => "demain",
        MsgKey::RelativeYesterday => "hier",
        MsgKey::RelativeInDays => "dans {0} jours",
        MsgKey::RelativeDaysAgo => "il y a {0} jours",
        MsgKey::DayLabel => "Jour {0}",
        MsgKey::RelativeDate => "{0} · {1}",

        MsgKey::NewNoteIn => "Nouvelle note dans {0}",
        MsgKey::NewFolderIn => "Nouveau dossier dans {0}",
        MsgKey::WelcomeVaultCreated => "Nouveau coffre créé dans {0} — créez votre première note",

        MsgKey::ScanProgress => "Analyse de {0}/{1} notes — Esc pour annuler",
        MsgKey::FocusHelp => "Masquer l’interface et garder la ligne du curseur centrée ({0})",
        MsgKey::FocusExitHelp => "Revenir à l’interface de la note ({0})",
        MsgKey::PaletteKanbanFolder => "Kanban · {0}",

        MsgKey::LinksNotUpdated => "Les liens dans {0} n’ont pas été mis à jour",
        MsgKey::RenamedLinksFailed => {
            "Note renommée, mais {0} fichier(s) contenant des liens entrants n’ont pas pu être réécrits\u{a0}: {1}"
        }
        MsgKey::MovedLinksFailed => {
            "Note déplacée, mais {0} fichier(s) contenant des liens entrants n’ont pas pu être réécrits\u{a0}: {1}"
        }
        MsgKey::NoteNotInVault => "La note {0} n’est plus dans ce coffre",
        MsgKey::CannotOpenWithoutNote => {
            "Impossible d’ouvrir {0}\u{a0}: aucune note n’est sélectionnée"
        }
        MsgKey::AttachmentNotInVault => {
            "La pièce jointe {0} n’est pas dans ce coffre — déposez-la ici ou corrigez l’intégration"
        }
        MsgKey::BrokenLinksCount => "{0} rompu(s)",

        MsgKey::NotAFile => "{0} n’est pas un fichier",
        MsgKey::PathGone => "{0} n’existe plus",
        MsgKey::TrayError => "Impossible d’ajouter l’icône de notification\u{a0}: {0}",
        MsgKey::LoginItemError => "Impossible de modifier l’élément de démarrage\u{a0}: {0}",
        MsgKey::OutsideVault => "{0} est en dehors du coffre",
        MsgKey::NotABoardInVault => "{0} n’est pas un tableau dans le coffre",
        MsgKey::AlreadyExists => "{0} existe déjà",
        MsgKey::NotATrashPath => "{0} n’est pas un chemin de la corbeille",
        MsgKey::NoLongerInTrash => "{0} n’est plus dans la corbeille",
        MsgKey::NotAFolder => "{0} n’est pas un dossier",
        MsgKey::BoardNameInvalid => "un nom de tableau {0}",
        MsgKey::BoardSceneKeyObject => "la scène {0} doit être un objet",
        MsgKey::BoardRenamedEmbedsFailed => {
            "Renommé, mais {0} note(s) qui l’intègrent n’ont pas pu être mises à jour\u{a0}: {1}"
        }
        MsgKey::AutostartRegFailed => "Impossible d’exécuter reg.exe\u{a0}: {0}",
        MsgKey::AutostartRegExit => "reg.exe s’est terminé avec {0}",
        MsgKey::KanbanQuitRefused => {
            "Kanban non enregistré et aucune copie de récupération n’a pu être écrite\u{a0}: {0}. Utilisez Enregistrer une copie ou corrigez le dossier, puis quittez à nouveau."
        }
        MsgKey::KanbanCouldNotSaveCopy => "impossible d’enregistrer une copie\u{a0}: {0}",
        MsgKey::CouldNotDo => "Impossible de {0}\u{a0}: {1}",
        MsgKey::CouldNotCreateBoard => "Impossible de créer un tableau\u{a0}: {0}",
        MsgKey::CouldNotCreateKanban => "Impossible de créer un kanban\u{a0}: {0}",
        MsgKey::CouldNotConvertKanban => "Impossible de convertir en kanban\u{a0}: {0}",

        MsgKey::BoardKeptAsIs => "{0} · {1} sans modification",
        MsgKey::BoardCouldNotRead => "Impossible de lire\u{a0}: {0}",
        MsgKey::BoardNoteLinkMissing => "{0} (manquant)",
        MsgKey::UnsavedSaves => "Non enregistré · {0} enregistre",
        MsgKey::NotSaved => "Non enregistré\u{a0}: {0}",
        MsgKey::LinkDescription => "Ouvrir {0}",
        MsgKey::NoteLabelRoot => "{0} (racine)",
        MsgKey::GraphNodeParent => "{0} ({1})",
        MsgKey::GraphStats => "{0} notes · {1} arêtes",

        MsgKey::KanbanFilterFrom => "depuis {0}",
        MsgKey::KanbanFilterTo => "jusqu’au {0}",
        MsgKey::KanbanFilterRange => "{0} – {1}",
        MsgKey::LaneActions => "Actions de la colonne\u{a0}: {0}",
        MsgKey::LaneControls => "Contrôles de {0}",
        MsgKey::CollapseLane => "Replier la colonne\u{a0}: {0}",
        MsgKey::ExpandLane => "Déplier la colonne\u{a0}: {0}",
        MsgKey::LaneCards => "Cartes de {0}",
        MsgKey::AddCardToLane => "Ajouter une carte à {0}",
        MsgKey::LaneLabel => "{0}, {1} cartes",
        MsgKey::LaneOverLimit => ", limite dépassée",
        MsgKey::LaneDone => ", colonne terminée",
        MsgKey::LaneRegionLabel => "Colonne {0}, {1} cartes",
        MsgKey::CollapsedLaneLabel => "{0}, {1} cartes, repliée",
        MsgKey::CollapsedCards => "Repliée · {0} cartes",
        MsgKey::DeleteLane => {
            "Supprimer «\u{a0}{0}\u{a0}»{1}\u{a0}? Tout texte de note qu’elle contient reste dans le fichier."
        }
        MsgKey::DeleteLaneText => {
            "Supprimer «\u{a0}{0}\u{a0}»{1}\u{a0}? Le texte de note que le tableau n’affiche pas sera aussi supprimé ({2}), à partir de «\u{a0}{3}\u{a0}»."
        }
        MsgKey::DeleteCardsOne => " et sa carte",
        MsgKey::DeleteCardsOther => " et ses {0} cartes",
        MsgKey::CardActions => "Actions de la carte\u{a0}: {0}",
        MsgKey::CardActionsEmpty => "Actions de la carte\u{a0}: carte vide",
        MsgKey::CardActionsInLane => "Actions de la carte\u{a0}: carte vide dans {0}",
        MsgKey::ChecklistProgress => "{0} sur {1} terminées",
        MsgKey::ChecklistLabel => "Liste de tâches, {0} sur {1} terminées",
        MsgKey::CardLabelInLane => "{0} · {1}\u{a0}: {2}",

        MsgKey::DeviceCode => {
            "Saisissez le code {0} sur {1}. Votre navigateur ouvre cette adresse\u{a0}; Muninn termine lorsque GitHub approuve."
        }
        MsgKey::BrowserFailed => {
            "Impossible d’ouvrir un navigateur. Ouvrez vous-même cette adresse pour continuer\u{a0}: {0}"
        }
        MsgKey::SignInFailed => {
            "La connexion ne s’est pas terminée\u{a0}: {0} Aucun jeton n’a été stocké."
        }
        MsgKey::TailnetHosting => {
            "Hébergement sur 127.0.0.1:{0}. Sur cet appareil, exécutez\u{a0}: {1}"
        }
        MsgKey::TailnetHostingReady => {
            "Hébergement actif. Exécutez une fois sur cet appareil\u{a0}: {0}. Utilisez Copier la clé pour transmettre la clé partagée à vos autres appareils."
        }
        MsgKey::SharedKeyCopied => {
            "Clé partagée copiée. Le presse-papiers sera vidé dans {0} secondes\u{a0}; collez la clé sur l’appareil qui rejoint."
        }
        MsgKey::SyncPreviousCredential => {
            "Impossible de supprimer l’identifiant de la connexion précédente\u{a0}: {0}"
        }
        MsgKey::SyncNothingSaved => "{0} Rien n’a été enregistré.",
        MsgKey::SyncKeyRotationFailed => "{0} L’ancienne clé fonctionne toujours.",
        MsgKey::SyncNowAfterError => {
            "{0} Reconnectez-vous avant de lancer Synchroniser maintenant."
        }
        MsgKey::SyncOffline => "Hors ligne\u{a0}: {0}",
        MsgKey::SyncRateLimitedAfter => "Trop de requêtes\u{a0}; réessayez dans {0} ms",
        MsgKey::SyncProviderError => "Erreur du fournisseur {0}\u{a0}: {1}",
        MsgKey::SyncAuthenticationNeeded => "Authentification requise\u{a0}: {0}",
        MsgKey::SyncPermissionDenied => "Autorisation refusée\u{a0}: {0}",
        MsgKey::SyncOauthScopeRejected => "Portée OAuth refusée\u{a0}: {0}",
        MsgKey::SyncRemoteScopeNotFound => "Portée distante introuvable\u{a0}: {0}",
        MsgKey::SyncRemoteChanged => "Le dépôt distant a changé\u{a0}: {0}",
        MsgKey::SyncOutcomeUnknown => "Résultat inconnu, réconciliez avant de réessayer\u{a0}: {0}",
        MsgKey::SyncListingIncomplete => {
            "Liste distante incomplète\u{a0}; refus de la traiter comme une suppression\u{a0}: {0}"
        }
        MsgKey::SyncUnsupported => "{0} non pris en charge\u{a0}: {1}",
        MsgKey::SyncLimitExceeded => "Limite de synchronisation dépassée\u{a0}: {0}",
        MsgKey::SyncCredentialsUnavailable => "Identifiants indisponibles\u{a0}: {0}",
        MsgKey::SyncDirtyBuffer => {
            "Enregistrez ou abandonnez les modifications non enregistrées dans {0} avant que la synchronisation puisse le modifier"
        }
        MsgKey::SyncLocalChanged => {
            "{0} a changé pendant la synchronisation\u{a0}; relancez Synchroniser maintenant pour recalculer le plan"
        }
        MsgKey::SyncNotConfigured => "La synchronisation n’est pas configurée\u{a0}: {0}",
        MsgKey::SyncFileError => "Erreur de fichier\u{a0}: {0}",

        MsgKey::RecoveredFor => "Modifications non enregistrées récupérées pour {0}",
        MsgKey::ImportedFilesOne => "{0} importé",
        MsgKey::ImportedFilesOther => "{0} importés",
        MsgKey::ImportJoin => " · {0}",
        MsgKey::ImportAttachmentsNote => " · les fichiers non Markdown restent des pièces jointes",
    }
}
