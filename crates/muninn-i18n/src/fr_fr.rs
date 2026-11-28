//! The `fr-FR` catalog.
//!
//! French (France), neutral software register. This exhaustive `match`
//! mirrors [`crate::en_us`] arm for arm and in the same order, so a key added
//! to the catalog fails `cargo check` here until it has French text.
//! Identifiers, placeholders, keycaps, product names and on-disk tokens stay
//! as written; see `docs/i18n.md` §5.8.
//!
//! Typography: prose uses the typographic apostrophe U+2019 (`l’application`)
//! and a no-break space U+00A0 before `:` `;` `!` `?` and inside `« »`. Those
//! characters are written as Rust escapes (`\u{a0}`) where they would be
//! invisible in review; they are copy, never part of a path or a token.
//!
//! The copy is machine-authored and awaits native/fluent review before release;
//! the terminology choices are recorded in `tests/french.rs` (the invariant
//! allowlist) and in the pull request's terminology notes.

use crate::key::Key;

/// The French text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping every catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Ajouter une carte",
        Key::Archive => "Archiver",
        Key::Today => "Aujourd’hui",
        Key::OverLimit => "Limite dépassée",
        Key::KeyboardShortcuts => "Raccourcis clavier",
        Key::Close => "Fermer",
        Key::Save => "Enregistrer",
        Key::Cancel => "Annuler",
        Key::Clear => "Effacer",
        Key::LinkedNote => "Note liée",
        Key::NoBoards => "Aucun tableau dans ce coffre pour l’instant",
        Key::HelpLastLogLines => "dernières lignes du journal\u{a0}:",
        Key::HelpGroupNavigation => "Navigation",
        Key::HelpNavFocusSearch => "Aller à la recherche",
        Key::HelpNavMoveHighlightedResult => "Déplacer le résultat sélectionné",
        Key::HelpNavOpenHighlightedResult => "Ouvrir le résultat sélectionné",
        Key::HelpNavBackInNoteHistory => "Reculer dans l’historique des notes",
        Key::HelpNavForwardInNoteHistory => "Avancer dans l’historique des notes",
        Key::HelpNavWriteReadGraphBoardKanban => {
            "Écriture / Lecture / Graphique / Tableau / Kanban"
        }
        Key::HelpNavFocusMode => {
            "Mode concentration\u{a0}: masquer l’interface et centrer le curseur"
        }
        Key::HelpNavCloseOutermostLayer => "Fermer la couche la plus externe",
        Key::HelpNavOpenCommandPalette => "Ouvrir la palette de commandes",
        Key::HelpGroupEdit => "Édition",
        Key::HelpEditFindInOpenNote => "Rechercher dans la note ouverte",
        Key::HelpEditFindInBoard => "Rechercher dans le tableau",
        Key::HelpEditHideCardsByTextTagDateOrState => {
            "Masquer les cartes par texte, étiquette, date ou état"
        }
        Key::HelpEditPerBoardOverrides => "Réglages par tableau, une clé à la fois",
        Key::HelpEditInsertWikilink => "Insérer un lien wiki",
        Key::HelpEditNewNote => "Nouvelle note",
        Key::HelpEditImportDocument => "Importer un document comme note",
        Key::HelpEditSaveNow => "Enregistrer maintenant (la sauvegarde automatique est active)",
        Key::HelpEditToggleWriteRead => "Basculer écriture / lecture",
        Key::HelpEditWikilinkCompletion => "Complétion des liens wiki",
        Key::HelpEditVaultTagCompletion => "Complétion des étiquettes du coffre",
        Key::HelpEditIndentFourSpaces => "Indenter de quatre espaces",
        Key::HelpEditPasteRichText => {
            "Le texte enrichi devient du Markdown\u{a0}; une URL sur une sélection devient un lien"
        }
        Key::HelpEditRenameSelectedNote => {
            "Renommer la note sélectionnée (dans Tableau\u{a0}: le tableau)"
        }
        Key::HelpEditDeleteSelectedNote => "Supprimer la note sélectionnée",
        Key::HelpGroupKanban => "Kanban",
        Key::HelpKanbanMoveThroughControls => "Parcourir les contrôles du tableau",
        Key::HelpKanbanMoveBetweenLaneCards => "Parcourir les cartes de la colonne active",
        Key::HelpKanbanMoveSelectionAcrossLanes => "Déplacer la sélection entre les colonnes",
        Key::HelpKanbanToggleCardDone => "Basculer la coche « terminé » de la carte sélectionnée",
        Key::HelpKanbanMoveSelectedCard => "Déplacer la carte sélectionnée",
        Key::HelpKanbanMoveCardToEdgeOrLane => {
            "La déplacer en haut, en bas ou dans une autre colonne"
        }
        Key::HelpKanbanOpenCardMenu => "Ouvrir le menu de la carte active",
        Key::HelpKanbanEditCard => "Modifier la carte",
        Key::HelpKanbanCloseMenuOrConfirmation => {
            "Fermer le menu, la confirmation ou la barre de conflit"
        }
        Key::HelpGroupGraph => "Graphique",
        Key::HelpGraphToggleGraph => "Basculer le graphique",
        Key::HelpGraphToggleGlobalLocal => "Basculer global / local",
        Key::HelpGraphMoveFocusRing => "Déplacer l’anneau de focus",
        Key::HelpGraphOpenFocusedNote => "Ouvrir la note active",
        Key::HelpGraphZoomInOut => "Zoom avant / arrière",
        Key::HelpGraphFitVisibleNodes => "Ajuster les nœuds visibles",
        Key::HelpGraphToggleIsolate => "Basculer l’isolement",
        Key::HelpGraphLeaveGraph => "Quitter le graphique",
        Key::HelpGroupBoard => "Tableau",
        Key::HelpBoardPanWithDrag => "Déplacer le tableau (ou faire glisser avec l’outil Main)",
        Key::HelpBoardZoomAtPointer => "Zoomer au pointeur",
        Key::HelpBoardZoomAtCanvasCentre => "Zoomer au centre du canevas",
        Key::HelpBoardPanTheBoard => "Déplacer le tableau",
        Key::HelpBoardFitTheDrawing => "Ajuster le dessin",
        Key::HelpBoardShapeTools => {
            "Outil rectangle, losange, ellipse, flèche, ligne ou main levée"
        }
        Key::HelpBoardDrawShape => "Dessiner cette forme\u{a0}; l’outil reste sélectionné",
        Key::HelpBoardPlaceText => "Placer du texte, puis cliquer et le saisir sur place",
        Key::HelpBoardEditTextLabel => "Écrire ou modifier son libellé de texte",
        Key::HelpBoardDropTheTool => "Relâcher l’outil (retour à la sélection)",
        Key::HelpBoardSelectElementOrToggle => "Sélectionner un élément / l’ajouter ou le retirer",
        Key::HelpBoardNudgeSelection => "Décaler la sélection de 1 unité (Shift\u{a0}: 5)",
        Key::HelpBoardDeleteSelection => "Supprimer la sélection",
        Key::HelpBoardSelectEveryElement => "Sélectionner tous les éléments",
        Key::HelpBoardLinkToNote => {
            "Lier les éléments sélectionnés à une note (vider retire le lien)"
        }
        Key::HelpBoardOpenLinkedNote => "Ouvrir la note liée de l’élément sélectionné",
        Key::HelpBoardOpenClickedLinkedNote => {
            "Ouvrir la note liée de l’élément cliqué (un clic sélectionne seulement)"
        }
        Key::HelpBoardUndoRedo => "Annuler / rétablir dessins, déplacements et suppressions",
        Key::HelpBoardSaveDrawing => "Enregistrer le dessin du tableau",
        Key::HelpBoardEscapeClearsSelection => {
            "Effacer la sélection, sinon relâcher l’outil, sinon quitter Tableau"
        }
        Key::HelpGroupSidebar => "Barre latérale",
        Key::HelpSidebarOpenThisHelp => "Ouvrir cette aide",
        Key::HelpSidebarOpenHelpWhenNotTyping => "Ouvrir l’aide hors saisie",
        Key::HelpSidebarMoveBetweenPanes => "Passer d’un volet à l’autre",
        Key::HelpSidebarMoveThroughAList => "Parcourir une liste",
        Key::HelpSidebarOpenFocusedRow => "Ouvrir la ligne active",
        Key::HelpSidebarNoteAndFolderActions => "Actions de la note et du dossier",
        Key::HelpSidebarSettings => {
            "Paramètres (coffres, thème, effets du graphique, valeurs par défaut du Kanban)"
        }
        Key::SettingsTabVaults => "Coffres",
        Key::SettingsTabEditor => "Éditeur",
        Key::SettingsTabAppearance => "Apparence",
        Key::SettingsTabGraph => "Graphique",
        Key::SettingsTabSync => "Synchronisation",
        Key::SettingsTabKanban => "Kanban",
        Key::SettingsVaultsOpenTitle => "Ouvrir un coffre",
        Key::SettingsVaultsOpenCaption => {
            "Collez le chemin d’un dossier ou créez un coffre dans un nouveau dossier."
        }
        Key::SettingsVaultsRecentTitle => "Coffres récents",
        Key::SettingsVaultsRecentCaption => {
            "Coffres ouverts récemment, les plus récents en premier."
        }
        Key::SettingsVaultsRecentEmpty => "Aucun coffre récent pour l’instant",
        Key::SettingsVaultsPathLabel => "Chemin du dossier du coffre",
        Key::SettingsVaultsPathHint => {
            "Collez un chemin de dossier\u{a0}; ~ correspond à votre dossier personnel."
        }
        Key::SettingsVaultsOpenAction => "Ouvrir le coffre",
        Key::SettingsVaultsNewAction => "Nouveau coffre",
        Key::SettingsEditorAutosaveTitle => "Délai de sauvegarde automatique",
        Key::SettingsEditorAutosaveCaption => {
            "Secondes d’inactivité avant l’enregistrement de la note ouverte."
        }
        Key::SettingsEditorTemplateTitle => "Modèle de note par défaut",
        Key::SettingsEditorTemplateCaption => {
            "Markdown inséré en tête de chaque nouvelle note. Laissez vide pour un simple titre."
        }
        Key::SettingsEditorTemplateLabel => "Modèle de note par défaut",
        Key::SettingsEditorTemplatePlaceholder => "Markdown inséré en tête de chaque nouvelle note",
        Key::SettingsAppearanceThemeTitle => "Thème",
        Key::SettingsAppearanceThemeFollowing => "Thème système d’Omarchy",
        Key::SettingsAppearanceLanguageTitle => "Langue",
        Key::SettingsAppearanceLanguageCaption => {
            "Système suit la langue de votre système d’exploitation et revient à l’anglais si aucun pack de langue ne correspond."
        }
        Key::SettingsAppearanceLanguageSystem => "Système",
        Key::SettingsAppearanceUnavailable => "Pas encore disponible sur macOS.",
        Key::SettingsAppearanceStateOn => "Activé.",
        Key::SettingsAppearanceStateOff => "Désactivé.",
        Key::SettingsAppearanceTrayTitle => "Icône de la zone de notification",
        Key::SettingsAppearanceTrayCaption => {
            "Une icône de notification avec Afficher Muninn, Nouvelle note, Synchroniser maintenant, l’état de synchronisation et Quitter."
        }
        Key::SettingsAppearanceTrayShow => "Afficher l’icône de notification",
        Key::SettingsAppearanceTrayStatusOn => {
            "Activé. Linux l’affiche si votre bureau dispose d’une zone de notification\u{a0}; GNOME requiert une extension AppIndicator."
        }
        Key::SettingsAppearanceCloseToTrayTitle => "Fermer dans la zone de notification",
        Key::SettingsAppearanceCloseToTrayNeedsTray => {
            "Nécessite l’icône de notification. Fermer la fenêtre quitte Muninn."
        }
        Key::SettingsAppearanceCloseToTrayOff => "Désactivé. Fermer la fenêtre quitte Muninn.",
        Key::SettingsAppearanceCloseToTrayOn => {
            "Activé. Fermer la fenêtre garde Muninn dans la zone de notification\u{a0}; Quitter Muninn à cet endroit quitte l’application. Si votre bureau n’affiche pas l’icône, laissez cette option désactivée."
        }
        Key::SettingsAppearanceCloseToTrayNoIcon => {
            "Activé, mais aucune icône de notification n’est disponible\u{a0}: fermer la fenêtre quitte Muninn."
        }
        Key::SettingsAppearanceStartupTitle => "Démarrage et raccourci",
        Key::SettingsAppearanceLoginTitle => "Lancer Muninn à l’ouverture de session",
        Key::SettingsAppearanceLoginOn => "Activé. Muninn s’ouvre à l’ouverture de votre session.",
        Key::SettingsAppearanceShortcutRegistering => "Enregistrement…",
        Key::SettingsAppearanceUpdateTitle => "Mises à jour",
        Key::SettingsAppearanceUpdateCaption => {
            "Muninn interroge GitHub pour savoir si une version stable plus récente existe. Aucune note ni donnée du coffre n’est envoyée."
        }
        Key::SettingsAppearanceUpdateOff => {
            "Désactivé. Muninn n’effectue aucune requête de mise à jour."
        }
        Key::SettingsAppearanceUpdateOn => {
            "Activé. Muninn interroge GitHub une fois par jour pour les versions stables."
        }
        Key::SettingsGraphReduceMotionTitle => "Réduire les animations",
        Key::SettingsGraphReduceMotionCaption => {
            "Fige la disposition du graphique, supprime les mouvements de caméra et rend statiques les effets de survol, de sélection et du graphique. Système suit le réglage de votre système d’exploitation."
        }
        Key::SettingsGraphReduceMotionEnv => {
            "Actuellement activé\u{a0}: MUNINN_REDUCED_MOTION est défini, ce qui prime sur ce choix."
        }
        Key::SettingsGraphReduceMotionSystemOn => {
            "Actuellement activé, selon le réglage du système."
        }
        Key::SettingsGraphReduceMotionSystemOff => {
            "Actuellement désactivé, selon le réglage du système."
        }
        Key::SettingsGraphReduceMotionOn => "Actuellement activé.",
        Key::SettingsGraphReduceMotionOff => "Actuellement désactivé.",
        Key::SettingsGraphEffectsTitle => "Effets du graphique",
        Key::SettingsGraphEffectsCaption => {
            "La même famille d’effets dessine la bordure de la sélection et les lignes qu’elle touche\u{a0}: Classique conserve les lignes simples, Électrique crée des arcs sur les deux, Désactivé dessine des lignes simples sans couches d’effet. Réduire les animations fige l’option choisie."
        }
        Key::SettingsGraphIntensityTitle => "Intensité des effets",
        Key::SettingsGraphIntensityCaption => {
            "Met à l’échelle les arcs d’Électrique et la brûlure à la suppression. Équilibré correspond à l’aspect d’origine. Classique ne dessine aucun arc et Désactivé ne dessine rien\u{a0}: le choix des arcs n’est donc visible qu’avec Électrique\u{a0}; la brûlure s’adapte quand même dans les deux cas. Réduire les animations fige toujours le résultat."
        }
        Key::SyncHeadingProvider => "Fournisseur de synchronisation",
        Key::SyncHeadingProviderCaption => {
            "Synchronisation manuelle uniquement. Un fournisseur par coffre. La déconnexion supprime les identifiants, pas les notes."
        }
        Key::SyncProviderGithubName => "GitHub",
        Key::SyncProviderGithubDescription => {
            "GitHub sélectionné. Utilisez un dépôt et une branche dédiés, pas le dépôt source de Muninn."
        }
        Key::SyncProviderDriveName => "Google Drive",
        Key::SyncProviderDriveShortName => "Drive",
        Key::SyncProviderDriveDescription => {
            "Google Drive sélectionné. La synchronisation conserve un historique de commits géré par l’application dans les données privées de Muninn de votre Drive, et non des fichiers Markdown modifiables."
        }
        Key::SyncProviderDriveCaption => {
            "Conservé dans les données privées de Muninn, dans votre Drive, et non dans un dossier visible. Utilisez le même nom de stockage sur chaque appareil."
        }
        Key::SyncProviderTailscaleName => "Tailscale",
        Key::SyncProviderTailscaleDescription => {
            "Tailscale sélectionné. Un appareil héberge un stockage de commits\u{a0}; les autres le rejoignent par nom MagicDNS sur votre tailnet."
        }
        Key::SyncFieldGithubOwner => "Propriétaire GitHub",
        Key::SyncFieldRepository => "Dépôt",
        Key::SyncFieldBranch => "Branche",
        Key::SyncFieldPrefix => "Préfixe",
        Key::SyncFieldGithubClientId => "ID client GitHub (facultatif)",
        Key::SyncFieldStoreName => "Nom du stockage",
        Key::SyncFieldGoogleClientId => "ID client Google (facultatif)",
        Key::SyncFieldLoopbackPort => "Port de bouclage",
        Key::SyncFieldAllowedLogins => {
            "Connexions Tailscale autorisées (facultatives, séparées par des virgules)"
        }
        Key::SyncFieldMagicDnsName => "Nom MagicDNS de l’hôte (device.tailnet.ts.net)",
        Key::SyncFieldSharedKey => "Clé partagée de l’hôte",
        Key::SyncActionConnect => "Connecter",
        Key::SyncActionSyncNow => "Synchroniser maintenant",
        Key::SyncActionDisconnect => "Déconnecter",
        Key::SyncTailnetRoleLabel => "Rôle Tailscale",
        Key::SyncTailnetRoleHost => "Héberger ici",
        Key::SyncTailnetRoleJoin => "Rejoindre un hôte",
        Key::SyncTailnetKeyLabel => "Clé partagée",
        Key::SyncTailnetKeyCopy => "Copier la clé",
        Key::SyncTailnetKeyShow => "Afficher la clé",
        Key::SyncTailnetKeyHide => "Masquer la clé",
        Key::SyncTailnetKeyRotate => "Renouveler la clé",
        Key::SyncStatusNotConnected => {
            "Non connecté. Choisissez un fournisseur et un dépôt distant explicite, puis cliquez sur Connecter."
        }
        Key::SyncStatusTailnetHost => {
            "Cet appareil héberge. Muninn sert le stockage de commits sur 127.0.0.1 tant qu’il est ouvert\u{a0}; tailscale serve le publie sur votre tailnet."
        }
        Key::SyncStatusTailnetJoin => {
            "Rejoignez un hôte. Saisissez le nom MagicDNS complet de l’hôte, puis collez la clé obtenue avec Copier la clé sur l’hôte."
        }
        Key::SyncStatusConnectedSharedKey => {
            "Connecté. La clé partagée est dans le magasin d’identifiants du système. Lancez Synchroniser maintenant quand vous êtes prêt."
        }
        Key::SyncStatusConnectedToken => {
            "Connecté. Le jeton est dans le magasin d’identifiants du système. Lancez Synchroniser maintenant quand vous êtes prêt."
        }
        Key::SyncStatusAskingGithubCode => "Demande d’un code de connexion à GitHub…",
        Key::SyncStatusOpeningGoogle => "Ouverture de la connexion Google dans votre navigateur…",
        Key::SyncStatusWaitingGoogle => {
            "Terminez la connexion à Google dans votre navigateur. Muninn attend la réponse sur 127.0.0.1."
        }
        Key::SyncStatusSignedInAfterDisconnect => {
            "La connexion s’est terminée après Déconnecter\u{a0}; son jeton a été supprimé."
        }
        Key::SyncStatusTailnetJoined => {
            "Rejoint. La clé partagée est dans le magasin d’identifiants du système. Synchroniser maintenant atteint l’hôte via votre tailnet."
        }
        Key::SyncStatusSyncStarted => "Synchronisation lancée. La fenêtre reste réactive.",
        Key::SyncStatusCancelling => {
            "Annulation. Une modification déjà acceptée par le fournisseur est conservée et réconciliée."
        }
        Key::SyncStatusNoteChangedWhileSyncing => {
            "La note ouverte a changé pendant la synchronisation\u{a0}; son texte n’a pas été écrasé."
        }
        Key::SyncStatusNothingConnected => {
            "Rien n’est connecté. Les notes locales n’ont pas été modifiées."
        }
        Key::SyncStatusDisconnected => {
            "Déconnecté. Les identifiants de cette liaison ont été supprimés. Les notes locales et distantes n’ont pas été supprimées."
        }
        Key::SyncStatusSaveBeforeSync => {
            "Enregistrez ou résolvez la note ouverte avant Synchroniser maintenant. Le texte non enregistré n’est ni envoyé ni écrasé."
        }
        Key::SyncStatusAlreadyRunning => "Une synchronisation est déjà en cours.",
        Key::SyncStatusFinishSignIn => {
            "Terminez d’abord la connexion, ou cliquez sur Déconnecter pour annuler."
        }
        Key::SyncStatusConnectProvider => {
            "Connectez un fournisseur pour ce coffre avant Synchroniser maintenant."
        }
        Key::SyncStatusWaitForRun => {
            "Attendez la fin de la synchronisation ou de la connexion en cours, ou cliquez sur Déconnecter pour l’annuler."
        }
        Key::SyncStatusWaitToRotateKey => {
            "Attendez la fin de la synchronisation en cours avant de renouveler la clé."
        }
        Key::SyncStatusKeyRotated => {
            "Clé renouvelée\u{a0}; l’ancienne ne fonctionne plus. Sur chaque appareil rejoint, cliquez sur Déconnecter et rejoignez à nouveau avec la nouvelle clé (Copier la clé)."
        }
        Key::SyncStatusCurrentConnectionKept => "La connexion actuelle n’a pas été modifiée.",
        Key::SyncErrorVaultNotCanonical => {
            "Le dossier de ce coffre ne peut pas être résolu en chemin canonique\u{a0}; il ne peut donc pas être lié à la synchronisation."
        }
        Key::SyncErrorGithubScope => {
            "Saisissez le propriétaire et le dépôt GitHub avant de vous connecter."
        }
        Key::SyncErrorLoopbackPort => {
            "Saisissez un port de bouclage entre 1024 et 65535 pour l’hôte."
        }
        Key::SyncErrorPasteSharedKey => {
            "Collez la clé partagée affichée sur l’hôte. Elle est stockée dans le magasin d’identifiants du système, pas dans les paramètres."
        }
        Key::SyncErrorNotTailnetHost => "Ce coffre n’héberge pas de stockage Tailscale.",
        Key::SyncErrorSharedKeyMissing => "La clé partagée est absente du magasin d’identifiants.",
        Key::SyncErrorTailnetKeyMissing => {
            "La clé partagée Tailscale est absente du magasin d’identifiants. Déconnectez-vous, puis reconnectez-vous."
        }
        Key::SyncErrorCancelled => "Synchronisation annulée",
        Key::SyncErrorTimedOutAfterSend => {
            "Délai dépassé après l’envoi d’une modification\u{a0}; réconciliez avant de réessayer"
        }
        Key::SyncErrorTimedOutBeforeResponse => "Délai dépassé avant la réponse du fournisseur",
        Key::SyncErrorRateLimited => "Trop de requêtes",
        Key::SyncErrorLeaseBusy => "Une autre instance de Muninn écrit dans ce coffre",
        Key::SyncErrorSettingsNotObject => {
            "Les paramètres de synchronisation n’étaient pas un objet, la synchronisation est donc désactivée."
        }
        Key::SyncErrorSettingsVersion => {
            "La version des paramètres de synchronisation n’est pas prise en charge, la synchronisation est donc désactivée."
        }
        Key::SyncErrorSettingsVaultList => {
            "La liste des coffres de synchronisation était absente, la synchronisation est donc désactivée."
        }
        Key::SyncErrorSettingsBindingInvalid => {
            "Une liaison de synchronisation était invalide et n’a pas été chargée. La synchronisation reste désactivée jusqu’à son remplacement."
        }
        Key::KanbanUiNotInsideVault => "le kanban n’est pas dans le coffre ouvert",
        Key::KanbanUiThis => "le kanban",
        Key::KanbanUiFieldRefused => {
            "Fermeture impossible\u{a0}: le kanban n’a pas pu être enregistré. Corrigez ce qui a bloqué l’enregistrement ou annulez les modifications, puis quittez à nouveau."
        }
        Key::ErrorSaveKanbanBeforeRestore => {
            "Enregistrez ou abandonnez le kanban actuel avant d’en restaurer un autre"
        }
        Key::PaletteTitle => "Palette de commandes",
        Key::PaletteEmpty => "Aucune note ni action ne correspond",
        Key::PaletteFooter => "↑↓ déplacer · Enter ouvrir · Esc fermer",
        Key::PaletteActionOpenLocalGraph => "Ouvrir le graphique local",
        Key::PaletteActionSwitchVault => "Changer de coffre",
        Key::PaletteActionOpenSettings => "Ouvrir les paramètres",
        Key::PaletteActionNewNote => "Nouvelle note",
        Key::PaletteActionTogglePreview => "Basculer écriture / lecture",
        Key::PaletteActionToggleGraph => "Basculer le graphique",
        Key::PaletteActionToggleBoard => "Basculer le tableau",
        Key::PaletteActionToggleKanban => "Basculer le Kanban",
        Key::PaletteActionToggleFocusMode => "Basculer le mode concentration",
        Key::PaletteActionNewKanban => "Nouveau Kanban",
        Key::PaletteActionConvertToKanban => "Convertir une note vide en Kanban",
        Key::PaletteActionImportDocument => "Importer un document…",
        Key::PaletteActionExportVault => "Exporter le coffre…",
        Key::PaletteActionImportVault => "Importer un coffre…",
        Key::PaletteHintGraphLocal => "Graphique · Local",
        Key::PaletteHintVaults => "Coffres",
        Key::PaletteHintHelp => "Aide",
        Key::PaletteHintKanban => "Kanban",
        Key::PaletteKeywordsOpenLocalGraph => "graphique local focus voisins portée",
        Key::PaletteKeywordsSwitchVault => "coffre ouvrir récent dossier changer",
        Key::PaletteKeywordsOpenSettings => "paramètres préférences options configuration aide",
        Key::PaletteKeywordsNewNote => "nouvelle note créer ajouter",
        Key::PaletteKeywordsTogglePreview => "écriture lecture aperçu éditeur mode",
        Key::PaletteKeywordsToggleGraph => "graphique global local mode",
        Key::PaletteKeywordsToggleBoard => {
            "tableau whiteboard excalidraw dessin canevas croquis mode"
        }
        Key::PaletteKeywordsToggleKanban => "kanban colonnes cartes tâches mode",
        Key::PaletteKeywordsToggleFocusMode => {
            "concentration zen écrire sans distraction masquer interface atténuer"
        }
        Key::PaletteKeywordsNewKanban => "kanban créer ajouter colonnes cartes tâches",
        Key::PaletteKeywordsConvertToKanban => {
            "kanban vide vierge convertir colonnes cartes tâches"
        }
        Key::PaletteKeywordsImportDocument => {
            "importer document docx doc word pdf office odt rtf epub csv"
        }
        Key::PaletteKeywordsExportVault => {
            "exporter coffre sauvegarde télécharger zip archive enregistrer copie"
        }
        Key::PaletteKeywordsImportVault => {
            "importer coffre restaurer téléverser zip archive ouvrir"
        }
        Key::PaletteSectionRecent => "Récents",
        Key::PaletteSectionNotes => "Notes",
        Key::PaletteSectionActions => "Actions",
        Key::PaletteSectionKanbans => "Kanbans",
        Key::BoardPageTitle => "Tableaux",
        Key::BoardPageNew => "Nouveau tableau",
        Key::BoardPageEmptyTitle => "Aucun tableau pour l’instant",
        Key::BoardPageEmptyDetail => "Créez-en un avec Nouveau tableau",
        Key::BoardPageEmptyHint => {
            "Un tableau est un fichier Excalidraw dans votre coffre. Créez-en un pour commencer à dessiner."
        }
        Key::BoardPageNoSelectionTitle => "Aucun tableau sélectionné",
        Key::BoardPageNoSelectionDetail => {
            "Choisissez un tableau dans le sélecteur ci-dessus pour voir son contenu"
        }
        Key::BoardPageTitleLabel => "Nom du tableau\u{a0}; double-cliquez pour le renommer",
        Key::BoardPageUnreadableTitle => "Ce tableau n’a pas pu être lu",
        Key::BoardPageNoOpenTitle => "Aucun tableau ouvert",
        Key::BoardPageNoOpenDetail => {
            "Choisissez un tableau dans le sélecteur ci-dessus ou créez-en un. Les tableaux sont des fichiers Excalidraw dans votre coffre."
        }
        Key::BoardPageEmptyCanvas => "Rien n’est encore dessiné sur ce tableau",
        Key::BoardPageHint => {
            "Clic/glisser sélectionner · Del supprimer · ⌘Z annuler · R D O A L P dessiner · H déplacer · ⌘L lier une note · ⌘S enregistrer · 0 ajuster"
        }
        Key::BoardPagePickerLabel => "Choisir un tableau",
        Key::BoardPagePickerHelp => "Ouvrir la liste des tableaux",
        Key::BoardPagePickerMenu => "Tableaux",
        Key::BoardToolToolbar => "Outils du tableau",
        Key::BoardToolHand => "Main",
        Key::BoardToolHandHelp => "Déplacer le tableau (H)",
        Key::BoardToolSelect => "Sélection",
        Key::BoardToolSelectHelp => "Sélectionner et déplacer des éléments (V)",
        Key::BoardToolRectangle => "Rectangle",
        Key::BoardToolRectangleHelp => "Dessiner un rectangle (R)",
        Key::BoardToolDiamond => "Losange",
        Key::BoardToolDiamondHelp => "Dessiner un losange (D)",
        Key::BoardToolEllipse => "Ellipse",
        Key::BoardToolEllipseHelp => "Dessiner une ellipse (O)",
        Key::BoardToolArrow => "Flèche",
        Key::BoardToolArrowHelp => "Dessiner une flèche (A)",
        Key::BoardToolLine => "Ligne",
        Key::BoardToolLineHelp => "Dessiner une ligne (L)",
        Key::BoardToolFreedraw => "Main levée",
        Key::BoardToolFreedrawHelp => "Dessiner à main levée (P ou X)",
        Key::BoardToolText => "Texte",
        Key::BoardToolTextHelp => "Placer du texte, puis saisir (T)",
        Key::BoardLinkTitle => "Lier une note",
        Key::BoardLinkHelp => "Lier les éléments sélectionnés à une note (⌘L)",
        Key::BoardLinkName => "Nom ou chemin de la note",
        Key::BoardLinkHint => "un nom ou un chemin du coffre · vide retire le lien",
        Key::BoardRenameName => "Nom du tableau",
        Key::BoardPaletteTitle => "Couleurs du tableau",
        Key::BoardPaletteHelp => "Couleurs de trait et de remplissage",
        Key::BoardPaletteOpen => "Palette de couleurs",
        Key::BoardPaletteClose => "Fermer la palette de couleurs",
        Key::BoardPaletteStroke => "Trait",
        Key::BoardPaletteFill => "Remplissage",
        Key::BoardPaletteSwatchBlack => "noir",
        Key::BoardPaletteSwatchGrey => "gris",
        Key::BoardPaletteSwatchRed => "rouge",
        Key::BoardPaletteSwatchOrange => "orange",
        Key::BoardPaletteSwatchYellow => "jaune",
        Key::BoardPaletteSwatchGreen => "vert",
        Key::BoardPaletteSwatchBlue => "bleu",
        Key::BoardPaletteSwatchPurple => "violet",
        Key::BoardPaletteSwatchTransparent => "transparent",
        Key::GraphPageEmpty => "Aucune note à dessiner",
        Key::GraphPageFindNode => "Rechercher un nœud du graphique",
        Key::GraphPageFindPlaceholder => "Rechercher un nœud · Enter",
        Key::GraphPageSelected => "SÉLECTIONNÉ",
        Key::GraphPageHint => {
            "Flèches cibler · Enter ouvrir · +/- zoom · 0/F ajuster · I isoler · Glisser déplacer · Molette zoom"
        }
        Key::GraphScopeGlobal => "Global",
        Key::GraphScopeLocal => "Local",
        Key::GraphActionFit => "Ajuster",
        Key::GraphActionReset => "Réinitialiser",
        Key::GraphActionIsolate => "Isoler",
        Key::GraphActionOpenNote => "Ouvrir la note",
        Key::KanbanSectionDatesTitle => "Dates et heures",
        Key::KanbanSectionDatesCaption => {
            "Comment les dates et les heures sont saisies, écrites et affichées."
        }
        Key::KanbanSectionTagsTitle => "Étiquettes",
        Key::KanbanSectionTagsCaption => {
            "Puces d’étiquette et liste des couleurs d’étiquette conservées."
        }
        Key::KanbanSectionMetadataTitle => "Métadonnées de la note liée",
        Key::KanbanSectionMetadataCaption => {
            "Clés affichées au pied d’une carte depuis sa première note liée."
        }
        Key::KanbanSectionArchiveTitle => "Archives",
        Key::KanbanSectionArchiveCaption => {
            "Ce que l’archivage écrit et combien de cartes les archives conservent."
        }
        Key::KanbanSectionBoardTitle => "Tableau et cartes",
        Key::KanbanSectionBoardCaption => {
            "Largeur des colonnes, édition des cartes et boutons d’en-tête."
        }
        Key::KanbanSectionAppTitle => "Application",
        Key::KanbanSectionAppCaption => {
            "Choix pour tous les tableaux\u{a0}; aucun réglage par tableau."
        }
        Key::KanbanChoiceEnter => "Enter",
        Key::KanbanChoiceShiftEnter => "Shift+Enter",
        Key::KanbanChoiceStart => "Début",
        Key::KanbanChoiceEnd => "Fin",
        Key::KanbanChoicePrepend => "Ajouter au début",
        Key::KanbanChoiceAppend => "Ajouter à la fin",
        Key::KanbanUiDefaultsNote => {
            "Le pied de réglages du tableau prime\u{a0}; sinon, ces valeurs s’appliquent à tous les tableaux."
        }
        Key::KanbanUiBoardSettings => "Réglages du tableau",
        Key::KanbanUiChangesWriteOnly => {
            "Les modifications écrivent uniquement cette clé dans le pied de réglages de cette note et peuvent être annulées."
        }
        Key::KanbanUiNoMetadataKeys => "Aucune clé de métadonnées",
        Key::KanbanUiNoTagColours => "Aucune couleur d’étiquette",
        Key::KanbanUiKeptAsWritten => "Conservé tel quel. L’affichage ne le réécrit jamais.",
        Key::KanbanUiSourceBoardIgnored => "Valeur du tableau ignorée",
        Key::KanbanUiSourceBoardOverride => "Remplacement par le tableau",
        Key::KanbanUiSourceAppIgnored => "Valeur de l’application ignorée",
        Key::KanbanUiSourceAppDefault => "Valeur par défaut de l’application",
        Key::KanbanUiSourceDefault => "Par défaut",
        Key::KanbanUiResetBuiltIn => "Réinitialiser à la valeur intégrée",
        Key::KanbanUiResetBoard => "Réinitialiser le tableau",
        Key::KanbanUiResetAppDefault => "Réinitialiser la valeur par défaut de l’application",
        Key::KanbanUiRemove => "Retirer",
        Key::KanbanUiAdd => "Ajouter",
        Key::KanbanUiContainsMarkdown => "Contient du Markdown",
        Key::KanbanUiNewMetadataKey => "Nouvelle clé de métadonnées",
        Key::KanbanUiUnlimited => "Illimité",
        Key::KanbanPageTitle => "Kanban",
        Key::KanbanPageViewSwitcher => "Vue Kanban",
        Key::KanbanPageFilter => "Filtrer",
        Key::KanbanPageClearFilters => "Effacer les filtres",
        Key::KanbanPageOpenMarkdown => "Ouvrir en Markdown",
        Key::KanbanPageResolveConflict => "Résoudre le conflit",
        Key::KanbanPageEmptyTitle => "Aucun kanban ouvert",
        Key::KanbanPageEmptyDetail => {
            if cfg!(target_os = "macos") {
                "Sélectionnez une note kanban, puis appuyez sur ⌘5 pour l’afficher comme tableau."
            } else {
                "Sélectionnez une note kanban, puis appuyez sur Ctrl+5 pour l’afficher comme tableau."
            }
        }
        Key::KanbanPageInvalidTitle => "Cette note ne peut pas être affichée comme kanban",
        Key::KanbanPageInvalidDetail => {
            "Son pied de réglages est incomplet ou n’est pas un JSON valide. Le fichier reste inchangé."
        }
        Key::KanbanLaneComplete => "Terminée",
        Key::KanbanLaneNew => "Nouvelle colonne",
        Key::KanbanLaneAdd => "Ajouter une colonne",
        Key::KanbanLaneTitle => "Titre de la colonne",
        Key::KanbanLaneCardLimit => "Limite de cartes",
        Key::KanbanLaneNoLimit => "Sans limite",
        Key::KanbanCardText => "Texte de la carte",
        Key::KanbanCardOpenLink => "Ouvrir le lien",
        Key::KanbanViewBoard => "Tableau",
        Key::KanbanViewList => "Liste",
        Key::KanbanViewTable => "Table",
        Key::KanbanViewBoardTab => "Vue tableau",
        Key::KanbanViewListTab => "Vue liste",
        Key::KanbanViewTableTab => "Vue table",
        Key::KanbanViewNoLanes => "Aucune colonne sur ce tableau",
        Key::KanbanViewNoCards => "Aucune carte",
        Key::KanbanViewNoMatchingCards => "Aucune carte correspondante",
        Key::KanbanViewCardsByLane => "Cartes par colonne",
        Key::KanbanViewColumns => "Colonne, Carte, Date, Étiquettes, Progression",
        Key::KanbanViewColumnLane => "Colonne",
        Key::KanbanViewColumnCard => "Carte",
        Key::KanbanViewColumnDate => "Date",
        Key::KanbanViewColumnTags => "Étiquettes",
        Key::KanbanViewColumnProgress => "Progression",
        Key::KanbanFilterFind => "Rechercher dans le tableau",
        Key::KanbanFilterTitle => "Filtrer les cartes",
        Key::KanbanFilterText => "Texte",
        Key::KanbanFilterDate => "Date",
        Key::KanbanFilterAnyDate => "Toute date",
        Key::KanbanFilterOverdue => "En retard",
        Key::KanbanFilterStatus => "État",
        Key::KanbanFilterAny => "Toutes",
        Key::KanbanFilterOpen => "Ouvertes",
        Key::KanbanFilterDone => "Terminées",
        Key::KanbanFilterClearAll => "Tout effacer",
        Key::KanbanFilterDateError => {
            "Un filtre de date utilise le format YYYY-MM-DD\u{a0}; la plage n’est pas appliquée."
        }
        Key::KanbanFilterPreviousMatch => "Correspondance précédente",
        Key::KanbanFilterNextMatch => "Correspondance suivante",
        Key::KanbanFilterCloseFind => "Fermer la recherche",
        Key::KanbanArchiveEmpty => "Aucune carte archivée.",
        Key::KanbanArchiveDelete => "Supprimer",
        Key::KanbanArchiveRestore => "Restaurer",
        Key::KanbanArchiveRestoreTo => "Restaurer dans…",
        Key::KanbanPickerAddDate => "Ajouter une date",
        Key::KanbanPickerAddTime => "Ajouter une heure",
        Key::KanbanPickerAddADate => "Ajouter une date",
        Key::KanbanPickerAddATime => "Ajouter une heure",
        Key::KanbanPickerHour => "Heure",
        Key::KanbanPickerMinute => "Minute",
        Key::KanbanPickerNow => "Maintenant",
        Key::KanbanPickerPreviousMonth => "Mois précédent",
        Key::KanbanPickerNextMonth => "Mois suivant",
        Key::KanbanPickerDays => "Jours",
        Key::KanbanSettingDateTriggerLabel => "Déclencheur de date",
        Key::KanbanSettingDateTriggerHelp => "Saisi avant une date, comme dans @{2026-11-12}.",
        Key::KanbanSettingTimeTriggerLabel => "Déclencheur d’heure",
        Key::KanbanSettingTimeTriggerHelp => "Saisi avant une heure, comme dans @@{14:30}.",
        Key::KanbanSettingDateFormatLabel => "Format de date",
        Key::KanbanSettingDateFormatHelp => {
            "Écrit dans un nouveau jeton de date\u{a0}; YYYY, MM, DD, MMMM et Do sont des jetons."
        }
        Key::KanbanSettingTimeFormatLabel => "Format d’heure",
        Key::KanbanSettingTimeFormatHelp => {
            "Écrit dans un nouveau jeton d’heure\u{a0}; HH, mm, h et A sont des jetons."
        }
        Key::KanbanSettingDateDisplayFormatLabel => "Format d’affichage de la date",
        Key::KanbanSettingDateDisplayFormatHelp => {
            "Comment une date est affichée au pied de la carte."
        }
        Key::KanbanSettingShowRelativeDateLabel => "Afficher la date relative",
        Key::KanbanSettingShowRelativeDateHelp => {
            "Ajoute aujourd’hui ou dans N jours à côté de la date."
        }
        Key::KanbanSettingHideDateInTitleLabel => "Masquer la date dans le titre",
        Key::KanbanSettingHideDateInTitleHelp => {
            "Garde les jetons de date et d’heure hors du texte principal de la carte."
        }
        Key::KanbanSettingLinkDateToDailyNoteLabel => "Lier la date à la note quotidienne",
        Key::KanbanSettingLinkDateToDailyNoteHelp => {
            "Fait de la date affichée un lien vers sa note quotidienne."
        }
        Key::KanbanSettingHideTagsInTitleLabel => "Masquer les étiquettes dans le titre",
        Key::KanbanSettingHideTagsInTitleHelp => {
            "Garde les jetons d’étiquette (#) hors du texte principal de la carte."
        }
        Key::KanbanSettingShowTagsAtBottomLabel => "Afficher les étiquettes en bas",
        Key::KanbanSettingShowTagsAtBottomHelp => {
            "Affiche les puces d’étiquette au pied de la carte."
        }
        Key::KanbanSettingTagColorsLabel => "Couleurs d’étiquette",
        Key::KanbanSettingTagColorsHelp => {
            "Conservé tel quel. Muninn n’a pas encore d’éditeur de couleurs."
        }
        Key::KanbanSettingLinkedPageMetadataLabel => "Métadonnées de la note liée",
        Key::KanbanSettingLinkedPageMetadataHelp => {
            "Clés affichées au pied de la carte depuis sa première note liée."
        }
        Key::KanbanSettingShowMissingMetadataLabel => "Afficher les métadonnées manquantes",
        Key::KanbanSettingShowMissingMetadataHelp => {
            "Affiche clé\u{a0}: — lorsque la note liée ou la clé est absente."
        }
        Key::KanbanSettingArchiveWithDateLabel => "Archiver avec la date",
        Key::KanbanSettingArchiveWithDateHelp => {
            "Ajoute la date au texte d’une carte lors de son archivage."
        }
        Key::KanbanSettingArchiveDateFormatLabel => "Format de date des archives",
        Key::KanbanSettingArchiveDateFormatHelp => {
            "Format de date utilisé lors de l’archivage\u{a0}; mêmes jetons que Format de date."
        }
        Key::KanbanSettingArchiveDatePositionLabel => "Position de la date des archives",
        Key::KanbanSettingArchiveDatePositionHelp => "Début ou fin du texte de la carte archivée.",
        Key::KanbanSettingArchiveDateSeparatorLabel => "Séparateur de date des archives",
        Key::KanbanSettingArchiveDateSeparatorHelp => {
            "Texte entre la carte et sa date\u{a0}; une valeur vide les joint."
        }
        Key::KanbanSettingMaxArchiveCardsLabel => "Nombre maximal de cartes archivées",
        Key::KanbanSettingMaxArchiveCardsHelp => {
            "Les cartes archivées les plus anciennes au-delà de ce nombre sont supprimées. 0 signifie illimité."
        }
        Key::KanbanSettingLaneWidthLabel => "Largeur des colonnes",
        Key::KanbanSettingLaneWidthHelp => {
            "Largeur des colonnes en points\u{a0}; le tableau limite l’affichage à 120–720."
        }
        Key::KanbanSettingNewLineTriggerLabel => "Déclencheur de nouvelle ligne",
        Key::KanbanSettingNewLineTriggerHelp => {
            "Dans l’éditeur de cartes, Enter valide et Shift+Enter insère un saut de ligne, ou l’inverse."
        }
        Key::KanbanSettingNewCardInsertionLabel => "Position des nouvelles cartes",
        Key::KanbanSettingNewCardInsertionHelp => {
            "Où le bouton + Ajouter une carte place la nouvelle carte."
        }
        Key::KanbanSettingShowCheckboxesLabel => "Afficher les cases à cocher",
        Key::KanbanSettingShowCheckboxesHelp => {
            "Affiche la case à cocher « terminé » de chaque carte."
        }
        Key::KanbanSettingShowFilterButtonLabel => "Bouton Filtrer",
        Key::KanbanSettingShowFilterButtonHelp => "Affiche le bouton Filtrer dans l’en-tête.",
        Key::KanbanSettingShowArchiveButtonLabel => "Bouton Archiver",
        Key::KanbanSettingShowArchiveButtonHelp => "Affiche le bouton Archiver dans l’en-tête.",
        Key::KanbanSettingShowOpenMarkdownButtonLabel => "Bouton Ouvrir en Markdown",
        Key::KanbanSettingShowOpenMarkdownButtonHelp => {
            "Affiche le bouton Ouvrir en Markdown dans l’en-tête."
        }
        Key::KanbanSettingNewNoteFolderLabel => "Dossier des nouvelles notes",
        Key::KanbanSettingNewNoteFolderHelp => {
            "Dossier utilisé par Nouvelle note depuis une carte\u{a0}; vide reprend le dossier de Nouvelle note."
        }
        Key::KanbanSettingNewNoteTemplateLabel => "Modèle des nouvelles notes",
        Key::KanbanSettingNewNoteTemplateHelp => {
            "Chemin d’une note dont le texte sert d’amorce à la nouvelle note\u{a0}; vide reprend le modèle de note de l’application."
        }
        Key::KanbanSettingKanbanAutosaveLabel => "Sauvegarde automatique du kanban",
        Key::KanbanSettingKanbanAutosaveHelp => {
            "Enregistre les modifications du kanban dès qu’elles sont au repos. Désactivé, l’enregistrement reste à ⌘S."
        }
        Key::StatusSaving => "Enregistrement…",
        Key::StatusSaved => "Enregistré",
        Key::StatusAutosaveOn => "Sauvegarde automatique activée",
        Key::StatusRetry => "Réessayer",
        Key::StatusUndo => "Annuler",
        Key::RecoveryRestore => "Restaurer",
        Key::RecoveryRestoreAll => "Tout restaurer",
        Key::RecoveryDismiss => "Ignorer",
        Key::RecoveryWriteEdits => "— Modifications en mode Écriture",
        Key::RecoveryKanbanEdits => "— Modifications du Kanban",
        Key::UpdateAvailableTitle => "Mise à jour disponible",
        Key::UpdateNow => "Mettre à jour",
        Key::UpdateDownloading => "Téléchargement…",
        Key::UpdateVerifying => "Vérification…",
        Key::UpdateExtracting => "Extraction…",
        Key::UpdateInstalling => "Installation…",
        Key::UpdateInterrupted => "la mise à jour s’est arrêtée de façon inattendue",
        Key::UpdateReleaseNotes => "Notes de version",
        Key::UpdateDismiss => "Ignorer",
        Key::UpdateHintAppImage => "Téléchargez la nouvelle AppImage depuis la page de la version.",
        Key::UpdateHintAur => "Mettez à jour avec votre assistant AUR.",
        Key::UpdateHintDeb => "Téléchargez et installez le nouveau .deb.",
        Key::UpdateHintTarball => "Relancez install.sh ou téléchargez la nouvelle archive tar.",
        Key::UpdateHintUnknown => "Consultez la page de la version.",
        Key::UpdateOpenFailed => "Impossible d’ouvrir la page de la version dans le navigateur.",
        Key::BrokenAllResolve => "Tous les liens sont valides",
        Key::BrokenTitle => "Liens rompus",
        Key::TopbarNoNoteSelected => "Aucune note sélectionnée",
        Key::TopbarOpenAsKanban => "Ouvrir comme kanban",
        Key::TrashTitle => "Corbeille",
        Key::TrashEmptyAction => "Vider la corbeille",
        Key::TrashExplainer => {
            "Les notes supprimées restent ici jusqu’à leur restauration ou leur suppression définitive. Au maximum 500 fichiers ou 100 Mo sont conservés\u{a0}; le dossier le plus ancien est supprimé en premier."
        }
        Key::TrayShow => "Afficher Muninn",
        Key::TrayNewNote => "Nouvelle note",
        Key::TraySyncNow => "Synchroniser maintenant",
        Key::TrayQuit => "Quitter Muninn",
        Key::TrayNotConnected => "Non connecté.",
        Key::TrayTooltip => "Muninn",
        Key::ErrorEnterVaultPath => "Saisissez le chemin du dossier du coffre",
        Key::ErrorEnterNewVaultPath => "Saisissez le chemin d’un dossier pour le nouveau coffre",
        Key::ErrorSaveNoteBeforeSwitch => {
            "Enregistrez ou résolvez la note actuelle avant de changer de coffre"
        }
        Key::ErrorSaveBoardBeforeSwitch => {
            "Enregistrez le tableau actuel avant de changer de coffre"
        }
        Key::ErrorSaveKanbanBeforeSwitch => {
            "Enregistrez le kanban actuel avant de changer de coffre"
        }
        Key::ErrorSaveNoteBeforeRestore => {
            "Enregistrez ou abandonnez la note actuelle avant d’en restaurer une autre"
        }
        Key::ErrorSaveBoardBeforeRestore => {
            "Enregistrez ou abandonnez le tableau actuel avant d’en restaurer un autre"
        }
        Key::ErrorSaveBoardBeforeRescan => {
            "Enregistrez le tableau actuel avant de relancer l’analyse du coffre"
        }
        Key::ErrorOpenerHintUnix => {
            "Installez xdg-utils (pour xdg-open) ou ouvrez le fichier manuellement."
        }
        Key::ErrorNoFileOpener => {
            "Aucun ouvreur de fichiers n’est documenté pour cette plateforme\u{a0}; ouvrez le fichier manuellement"
        }
        Key::ErrorOpenerHintMacos => {
            "La commande `open` du système a échoué\u{a0}; ouvrez le fichier depuis le Finder."
        }
        Key::ErrorOpenerHintWindows => {
            "L’ouvreur du système a échoué\u{a0}; ouvrez le fichier depuis l’Explorateur."
        }
        Key::ErrorOpenerHintOther => "Ouvrez le fichier manuellement.",
        Key::ErrorDocumentBoard => "tableau",
        Key::ErrorDocumentKanban => "kanban",
        Key::ImportDocument => "Importer un document…",
        Key::ImportDocumentHelp => "Importer un document comme nouvelle note",
        Key::ImportDialogTitle => "Importer un document",
        Key::ImportDialogFilterDocuments => "Documents",
        Key::ErrorImportDialogUnavailable => {
            "Aucune boîte de dialogue de fichiers sur ce bureau — déposez le document sur la fenêtre."
        }
        Key::StatusTrashEmptied => "Corbeille vidée",
        Key::StatusLinkCopied => "Lien vers la carte copié",
        Key::ConflictExternalChange => "Modification externe détectée",
        Key::ConflictFileDeleted => "fichier supprimé en dehors de Muninn",
        Key::ConflictChangedOnDisk => "Modifié sur le disque par une autre application",
        Key::ConflictMissingOnDisk => "Supprimé ou renommé sur le disque",
        Key::ConflictKeepMine => "Garder la mienne",
        Key::ConflictLoadTheirs => "Charger l’autre version",
        Key::ConflictSaveCopy => "Enregistrer une copie",
        Key::ConflictDiscard => "Abandonner",
        Key::CommonAppName => "Muninn",
        Key::CommonDelete => "Supprimer",
        Key::CommonEdit => "Modifier",
        Key::CommonMove => "Déplacer",
        Key::CommonNewNote => "Nouvelle note",
        Key::CommonOpen => "Ouvrir",
        Key::CommonRename => "Renommer",
        Key::CommonResults => "Résultats",
        Key::CommonVaultRoot => "Racine du coffre",
        Key::CommonDismiss => "Ignorer",
        Key::CommonOn => "Activé",
        Key::CommonOff => "Désactivé",
        Key::SidebarNoNotesTitle => "Aucune note pour l’instant",
        Key::SidebarNoNotesHint => "Ajoutez des fichiers .md ici ou créez-en un ci-dessous.",
        Key::SidebarNoMatchTitle => "Aucune note ne correspond",
        Key::SidebarNoMatchHint => "Essayez une autre recherche.",
        Key::SidebarSearch => "Rechercher des notes",
        Key::SidebarLinksOk => "liens valides",
        Key::SidebarKanbans => "KANBANS",
        Key::SidebarBoards => "TABLEAUX",
        Key::SidebarTheVaultRoot => "la racine du coffre",
        Key::SettingsTitle => "Paramètres",
        Key::TopbarSettingsHelp => "Paramètres (Ctrl+,)",
        Key::TopbarHideSidebar => "Masquer la barre latérale",
        Key::TopbarShowSidebar => "Afficher la barre latérale",
        Key::SettingsChangesSave => "Les modifications sont enregistrées automatiquement.",
        Key::SettingsClose => "Fermer les paramètres (Esc)",
        Key::SettingsSection => "Section des paramètres",
        Key::SettingsAutosave => "Sauvegarde automatique",
        Key::SettingsDiagnostics => "Diagnostic",
        Key::SettingsCopyDiagnostics => "Copier le diagnostic",
        Key::SettingsReportIssue => "Signaler un problème sur GitHub",
        Key::SettingsThemeOmarchy => "Omarchy",
        Key::SettingsThemeSystem => "Système",
        Key::SettingsThemeDark => "Sombre",
        Key::SettingsThemeLight => "Clair",
        Key::SettingsMotionSystem => "Système",
        Key::SettingsMotionOn => "Activé",
        Key::SettingsMotionOff => "Désactivé",
        Key::SettingsEffectsClassic => "Classique",
        Key::SettingsEffectsElectric => "Électrique",
        Key::SettingsEffectsOff => "Désactivé / Minimal",
        Key::SettingsIntensitySubtle => "Subtil",
        Key::SettingsIntensityBalanced => "Équilibré",
        Key::SettingsIntensityIntense => "Intense",
        Key::WelcomeGraphLabel => "Ouvrir le graphique des liens",
        Key::WelcomeGraphHelp => "Voyez comment les notes se connectent par les [[liens]]",
        Key::WelcomeGraphPill => "[[une note]] → graphique",
        Key::PreviewEmptyNote => "Cette note est vide. Écrivez quelque chose dans l’éditeur.",
        Key::PreviewStartWriting => "Créez une note pour commencer à écrire.",
        Key::PreviewEditor => "Éditeur de notes",
        Key::PreviewEditorPlaceholder => "Commencez à écrire. Tapez [[ pour lier une autre note.",
        Key::PreviewCompletions => "Complétions",
        Key::PreviewAttachments => "PIÈCES JOINTES",
        Key::PreviewAttachmentsNote => "Les fichiers restent hors de l’index des notes",
        Key::PreviewOpen => "Ouvrir",
        Key::PreviewOpenBoard => "Ouvrir le tableau",
        Key::PreviewMissingFile => "fichier manquant",
        Key::PreviewBoardKind => "tableau",
        Key::PreviewAttachmentKind => "pièce jointe",
        Key::PreviewMissing => "manquant",
        Key::PreviewNoOutgoing => "Aucun lien sortant",
        Key::PreviewNoBacklinks => "Aucun rétrolien pour l’instant",
        Key::PreviewLinks => "LIENS",
        Key::PreviewOutgoing => "SORTANTS",
        Key::PreviewBacklinks => "RÉTROLIENS",
        Key::PreviewGraph => "GRAPHIQUE",
        Key::PreviewGraphHint => "Voyez cette note et tout ce qu’elle touche.",
        Key::PreviewFind => "Rechercher dans la note",
        Key::StartupScanning => "Analyse de votre coffre",
        Key::StartupScanCancelled => "Analyse annulée",
        Key::StartupCouldNotOpen => "Impossible d’ouvrir le coffre",
        Key::StartupRetryHint => {
            "Réessayer relance l’analyse du coffre, ou choisissez un autre dossier dans Paramètres (⌘,)."
        }
        Key::StartupPickFolderHint => {
            "Choisissez un autre dossier dans Paramètres (⌘,), ou passez --vault / définissez MUNINN_VAULT."
        }
        Key::ModeWrite => "Écriture",
        Key::ModeRead => "Lecture",
        Key::ModeGraph => "Graphique",
        Key::ModeBoard => "Tableau",
        Key::ModeKanban => "Kanban",
        Key::FocusExit => "Quitter le mode concentration",
        Key::FocusEnter => "Mode concentration",
        Key::MenuOpenExternally => "Ouvrir à l’extérieur",
        Key::MenuReveal => "Afficher dans le gestionnaire de fichiers",
        Key::MenuMoveTo => "Déplacer vers…",
        Key::MenuNewNoteHere => "Nouvelle note ici",
        Key::MenuNewFolder => "Nouveau dossier…",
        Key::MenuInsertBefore => "Insérer une carte avant",
        Key::MenuInsertAfter => "Insérer une carte après",
        Key::MenuDuplicate => "Dupliquer",
        Key::MenuMarkOpen => "Marquer comme ouverte",
        Key::MenuMarkDone => "Marquer comme terminée",
        Key::MenuChangeDate => "Modifier la date…",
        Key::MenuAddDate => "Ajouter une date…",
        Key::MenuChangeTime => "Modifier l’heure…",
        Key::MenuAddTime => "Ajouter une heure…",
        Key::MenuSplitCard => "Scinder la carte",
        Key::MenuCopyLink => "Copier le lien vers la carte",
        Key::MenuNewNoteFromCard => "Nouvelle note depuis la carte",
        Key::MenuMarkAllDone => "Marquer toutes les cartes comme terminées",
        Key::MenuNoteActions => "Actions de la note",
        Key::MenuFolderActions => "Actions du dossier",
        Key::MenuBoardActions => "Actions du tableau",
        Key::MenuCardActions => "Actions de la carte",
        Key::MenuLaneActions => "Actions de la colonne",
        Key::MenuArchiveActions => "Actions des archives",
        Key::MenuMoveToLane => "Déplacer vers la colonne",
        Key::LinkImpactHere => "ici",
        Key::LinkImpactIntoFolder => "dans ce dossier",
        Key::LinkThisNote => "Cette note",
        Key::LinkNameFallback => "Lien",
        Key::RenameRewriteLinks => "Réécrire les liens vers le nouveau nom",
        Key::RenameLeaveDangling => "Laisser les liens non résolus",
        Key::DeleteNoteQuestion => "Supprimer cette note\u{a0}?",
        Key::DeleteFolderQuestion => "Supprimer le dossier\u{a0}? (vide uniquement)",
        Key::NewNoteName => "Nom de la nouvelle note",
        Key::NoteNamePlaceholder => "Nom de la note",
        Key::RenameNoteName => "Renommer la note",
        Key::FolderNamePlaceholder => "Nom du dossier",
        Key::NewFolderName => "Nom du nouveau dossier",
        Key::RenameFolderName => "Renommer le dossier",
        Key::PaletteMissingNote => "Note manquante",
        Key::PaletteKanbanRoot => "Kanban · Racine du coffre",
        Key::BoardRenameSaveFirst => {
            "Enregistrez d’abord le tableau\u{a0}: il n’a pas pu être enregistré"
        }
        Key::KanbanFilterToday => "aujourd’hui",
        Key::KanbanFilterOverdueWord => "en retard",
        Key::KanbanFilterOpenWord => "ouverte",
        Key::KanbanFilterDoneWord => "terminée",
        Key::KanbanDailyNoteLink => "Ouvrir la note quotidienne de la date",
        Key::KanbanLaneMoveUp => "Monter",
        Key::KanbanLaneMoveTop => "Déplacer en haut",
        Key::KanbanLaneMoveDown => "Descendre",
        Key::KanbanLaneMoveBottom => "Déplacer en bas",
        Key::KanbanLaneMovePreviousList => "Déplacer vers la colonne précédente",
        Key::KanbanLaneMoveNextList => "Déplacer vers la colonne suivante",
        Key::KanbanLaneMoveToList => "Déplacer vers une colonne…",
        Key::KanbanLaneInsertBefore => "Insérer une colonne avant",
        Key::KanbanLaneInsertAfter => "Insérer une colonne après",
        Key::KanbanLaneMoveLeft => "Déplacer la colonne vers la gauche",
        Key::KanbanLaneMoveRight => "Déplacer la colonne vers la droite",
        Key::KanbanLaneRename => "Renommer la colonne",
        Key::KanbanLaneLimit => "Limite de cartes…",
        Key::KanbanLaneMarkNotDone => "Marquer la colonne comme non terminée",
        Key::KanbanLaneMarkDone => "Marquer la colonne comme terminée",
        Key::KanbanLaneSortText => "Trier par texte",
        Key::KanbanLaneSortDate => "Trier par date",
        Key::KanbanLaneSortTags => "Trier par étiquettes",
        Key::KanbanLaneDelete => "Supprimer la colonne",
        Key::KanbanLaneDeleteCards => "Supprimer la colonne et ses cartes",
        Key::KanbanLaneDeleteText => "Supprimer la colonne et son texte de note",
        Key::KanbanCardFallback => "Carte",
        Key::KanbanCardMarkDone => "Marquer comme terminée",
        Key::KanbanCardMarkOpen => "Marquer comme non terminée",
        Key::KanbanCardChecklistItem => "Élément de la liste de tâches",
        Key::KanbanEditAddCard => "ajouter la carte",
        Key::KanbanEditEditCard => "modifier la carte",
        Key::KanbanEditDeleteCard => "supprimer la carte",
        Key::KanbanEditDuplicateCard => "dupliquer la carte",
        Key::KanbanEditSplitCard => "scinder la carte",
        Key::KanbanEditChangeDoneMark => "modifier la coche « terminé »",
        Key::KanbanEditToggleChecklistItem => "basculer l’élément de la liste de tâches",
        Key::KanbanEditMarkCardsDone => "marquer les cartes comme terminées",
        Key::KanbanEditAddLane => "ajouter la colonne",
        Key::KanbanEditRenameLane => "renommer la colonne",
        Key::KanbanEditClearLimit => "effacer la limite",
        Key::KanbanEditSetLimit => "définir la limite",
        Key::KanbanEditDeleteLane => "supprimer la colonne",
        Key::KanbanEditChangeDoneLane => "changer la colonne des tâches terminées",
        Key::KanbanEditSortLane => "trier la colonne",
        Key::KanbanEditCollapseLane => "replier la colonne",
        Key::KanbanEditMoveCard => "déplacer la carte",
        Key::KanbanEditChangeDate => "modifier la date",
        Key::KanbanEditChangeTime => "modifier l’heure",
        Key::KanbanEditClearDate => "effacer la date",
        Key::KanbanEditClearTime => "effacer l’heure",
        Key::KanbanEditChangeBoardSetting => "modifier le réglage du tableau",
        Key::KanbanEditResetBoardSetting => "réinitialiser le réglage du tableau",
        Key::KanbanEditCopyCardLink => "copier le lien de la carte",
        Key::KanbanEditMakeNoteFromCard => "créer une note à partir de la carte",
        Key::KanbanEditArchiveCard => "archiver la carte",
        Key::KanbanEditRestoreCard => "restaurer la carte archivée",
        Key::KanbanEditDeleteArchivedCard => "supprimer la carte archivée",
        Key::KanbanStatusLaneRefused => "la colonne en cours de modification a été refusée",
        Key::KanbanStatusCardRefused => "la carte en cours de modification a été refusée",
        Key::KanbanStatusNotInsideVault => "le kanban n’est pas dans le coffre ouvert",
        Key::KanbanStatusNoteGone => "la note n’est plus dans le coffre",
        Key::KanbanStatusFileUnchecked => "le fichier n’a pas pu être vérifié",
        Key::KanbanStatusFileRemoved => "le fichier a été supprimé",
        Key::KanbanStatusFileChanged => "le fichier a changé sur le disque",
        Key::KanbanTimeInvalid => "Saisissez une heure entre 0 et 23 et une minute entre 0 et 59",
        Key::KanbanLaneLimitInvalid => "La limite de cartes doit être un nombre entier",
        Key::VaultEmptyFolderName => "nom de dossier vide",
        Key::VaultCannotRenameRoot => "impossible de renommer la racine",
        Key::VaultCannotDeleteRoot => "impossible de supprimer la racine",
        Key::VaultDestinationExists => "la destination existe déjà",
        Key::BoardParseNotObject => "un tableau doit être un objet JSON",
        Key::BoardParseNotExcalidraw => {
            "ce n’est pas une scène Excalidraw (le type n’est pas «\u{a0}excalidraw\u{a0}»)"
        }
        Key::BoardParseVersionNumber => "la version de la scène doit être un nombre entier",
        Key::BoardParseSourceString => "la source de la scène doit être une chaîne",
        Key::BoardParseElementsArray => "les éléments de la scène doivent être un tableau JSON",
        Key::BoardStatusUnreadable => "le tableau n’a pas pu être lu, il n’est donc pas écrit",
        Key::BoardStatusOtherVault => "le tableau appartient à un autre coffre",
        Key::BoardStatusInsideVault => "le tableau n’est pas dans le coffre ouvert",
        Key::KanbanEditMoveLane => "déplacer la colonne",
        Key::AutostartPathNotUtf8 => "le chemin de l’exécutable n’est pas en UTF-8",
        Key::AutostartPathLineBreak => "le chemin de l’exécutable contient un saut de ligne",
        Key::NoteNoLongerIndexed => "la note n’est plus indexée",
        Key::TrashEmpty => "La corbeille est vide",
        Key::CoreBoardNameEmpty => "ne peut pas être vide",
        Key::CoreBoardNameSeparators => "ne peut pas contenir / ni \\",
        Key::CoreBoardNameHidden => "ne peut pas commencer par un point",
        Key::CoreBoardNameForbidden => {
            "ne peut pas contenir les caractères «\u{a0}: * ? \" < > |\u{a0}» ni de caractères de contrôle"
        }
        Key::CoreBoardNameTrailingDot => "ne peut pas se terminer par un point",
        Key::CoreBoardNameReserved => "ne peut pas être un nom de périphérique réservé",
        Key::MenuLockNote => "Verrouiller la note…",
        Key::MenuUnlockNote => "Déverrouiller la note…",
        Key::LockNoteTitle => "Verrouiller la note",
        Key::UnlockNoteTitle => "Déverrouiller la note",
        Key::LockPassword => "Mot de passe",
        Key::LockConfirmPassword => "Confirmer le mot de passe",
        Key::LockLostPasswordWarning => {
            "Il n’existe aucune réinitialisation de mot de passe, clé de récupération ni compte \
             Muninn. Si vous perdez ce mot de passe, la note est perdue. Les sauvegardes et \
             l’historique de synchronisation antérieurs peuvent encore contenir le texte en clair."
        }
        Key::LockSubmit => "Verrouiller",
        Key::UnlockSubmit => "Déverrouiller",
        Key::LockPasswordMismatch => "Les mots de passe ne correspondent pas.",
        Key::LockPasswordEmpty => "Saisissez un mot de passe.",
        Key::LockedNoteHeading => "Cette note est verrouillée",
        Key::LockedNoteBody => {
            "Déverrouillez-la avec son mot de passe pour la lire ou la modifier."
        }
        Key::LockedNoteRecoveryOff => {
            "La récupération après plantage est désactivée pour une note verrouillée ; les \
             modifications non enregistrées dépendent de l’enregistrement automatique."
        }
        Key::LockedNoteBadge => "Note verrouillée",
        Key::UnlockWrongPassword => "Ce mot de passe n’a pas ouvert la note.",
        Key::UnlockUnsupportedVersion => {
            "Cette note a été verrouillée par une version plus récente de Muninn."
        }
        Key::UnlockMalformed => "Cette note verrouillée est endommagée.",
        Key::UnlockWrongNote => "Cette note verrouillée appartient à un autre nom de fichier.",
        Key::UnlockRollback => {
            "Cette note est plus ancienne que la dernière version vue par Muninn."
        }
        Key::UnlockFailed => "La note n’a pas pu être déverrouillée.",
        Key::LockRefusedSave => {
            "Cette note est verrouillée. Déverrouillez-la pour conserver vos modifications."
        }
        Key::LockRefusedRename => {
            "Déverrouillez cette note avant de la renommer ou de la déplacer."
        }
        Key::LockRefusedFolderRename => {
            "Déverrouillez ou supprimez les notes verrouillées de ce dossier avant de le renommer."
        }
        Key::LockConflictSaveCopy => {
            "L’enregistrement d’une copie d’une note verrouillée n’est pas pris en charge. \
             Déverrouillez-la d’abord."
        }
        Key::HelpGroupLock => "Notes verrouillées",
        Key::HelpLockLock => {
            "Verrouiller la note : ajoutez un mot de passe ; le fichier devient du texte chiffré."
        }
        Key::HelpLockUnlock => {
            "Déverrouiller la note : saisissez le mot de passe pour cette session."
        }
        Key::HelpLockLost => {
            "Il n’y a ni réinitialisation de mot de passe ni clé de récupération. Un mot de passe \
             perdu perd la note."
        }
        Key::HelpLockRecovery => {
            "La récupération après plantage est désactivée pour les notes verrouillées ; \
             l’enregistrement automatique écrit toujours l’enveloppe."
        }
        Key::HelpLockRename => {
            "Déverrouillez une note verrouillée avant de la renommer ou de la déplacer."
        }
        Key::PaletteActionLockNote => "Verrouiller la note",
        Key::PaletteActionUnlockNote => "Déverrouiller la note",
        Key::PaletteKeywordsLockNote => "chiffrer mot de passe privé protéger",
        Key::PaletteKeywordsUnlockNote => "ouvrir déchiffrer mot de passe",
        Key::WebPersistDenied => {
            "Ce navigateur n’a pas accordé de stockage persistant : le coffre fonctionne, mais le navigateur peut l’évincer sous pression de stockage."
        }
        Key::WebReadOnlySample => {
            "Cet onglet en lecture seule affiche l’exemple intégré jusqu’à ce que l’onglet en écriture l’enregistre."
        }
        Key::WebVaultReadOnly => {
            "Lecture seule\u{a0}: un autre onglet Muninn détient le verrou d’écriture de ce coffre du navigateur."
        }
        Key::WebVaultNoLock => {
            "Lecture seule\u{a0}: ce navigateur n’a pas de Web Locks, donc un second onglet pourrait écraser celui-ci."
        }
        Key::WebExportUnavailable => {
            "L’exportation est indisponible tant que le stockage du navigateur n’est pas ouvert."
        }
        Key::WebExportFailed => "Le coffre n’a pas pu être exporté.",
        Key::WebExportDone => "L’exportation du coffre a été téléchargée.",
        Key::WebImportUnavailable => {
            "L’importation est indisponible tant que le stockage du navigateur n’est pas ouvert."
        }
        Key::WebImportFailed => "Le coffre n’a pas pu être importé.",
        Key::WebImportDone => "L’importation du coffre est terminée.",
        Key::WebImportRefusedDirty => {
            "Importation refusée\u{a0}: enregistrez ou abandonnez d’abord les modifications non enregistrées de la note ouverte."
        }
        Key::WebImportUnreadable => "Ce fichier n’est pas une archive de coffre Muninn.",
        Key::WebArchiveFileType => "Archive ZIP",
    }
}
