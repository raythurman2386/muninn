//! The `pt-BR` template table for [`MsgKey`].
//!
//! Mirrors [`crate::messages`] arm for arm and in the same order. Every
//! placeholder keeps its position in a natural Brazilian Portuguese sentence;
//! the one/other pairs are separate rows, exactly as in English, so a count of
//! 0 picks the plural row (`0 notas`) and 1 picks the singular (`1 nota`).
//!
//! `ImportedFilesOne`/`ImportedFilesOther` agree with the counted phrase they
//! receive (`1 arquivo` / `2 arquivos`), so the participle matches the noun:
//! `1 arquivo importado`, `2 arquivos importados`.
//!
//! The copy is reviewed by a native or fluent speaker before release.

use crate::msg::MsgKey;

/// The Brazilian Portuguese template for `key`.
///
/// Private: callers go through [`crate::phrase`] or [`crate::phrase_current`],
/// which substitute the arguments and pick the language. Keeping every table
/// behind those functions is what makes a second language a change to this
/// crate rather than to call sites.
pub(crate) fn template(key: MsgKey) -> &'static str {
    match key {
        MsgKey::FilesSkippedOne => "{0} arquivo ignorado",
        MsgKey::FilesSkippedOther => "{0} arquivos ignorados",
        MsgKey::NoteOne => "{0} nota",
        MsgKey::NoteOther => "{0} notas",
        MsgKey::BoardOne => "{0} quadro",
        MsgKey::BoardOther => "{0} quadros",
        MsgKey::TargetOne => "{0} destino",
        MsgKey::TargetOther => "{0} destinos",
        MsgKey::ReferrerOne => "{0} referenciador",
        MsgKey::ReferrerOther => "{0} referenciadores",
        MsgKey::WordOne => "{0} palavra",
        MsgKey::WordOther => "{0} palavras",
        MsgKey::LinkOne => "{0} link",
        MsgKey::LinkOther => "{0} links",
        MsgKey::BacklinkOne => "{0} backlink",
        MsgKey::BacklinkOther => "{0} backlinks",
        MsgKey::GraphOutgoingLinkOne => "{0} link de saída",
        MsgKey::GraphOutgoingLinkOther => "{0} links de saída",
        MsgKey::ResultOne => "{0} resultado",
        MsgKey::ResultOther => "{0} resultados",
        MsgKey::ElementOne => "{0} elemento",
        MsgKey::ElementOther => "{0} elementos",
        MsgKey::ArchivedCardOne => "{0} cartão arquivado",
        MsgKey::ArchivedCardOther => "{0} cartões arquivados",
        MsgKey::LineOne => "{0} linha",
        MsgKey::LineOther => "{0} linhas",
        MsgKey::FileOne => "{0} arquivo",
        MsgKey::FileOther => "{0} arquivos",

        MsgKey::StatusCounts => "{0} · {1} · {2}",
        MsgKey::TrashSummary => "Lixeira · {0} · {1}",
        MsgKey::NoteLinks => "{0} · {1}",
        MsgKey::NoLinks => "sem links",
        MsgKey::NoteLinksRoot => "raiz do cofre · {0}",
        MsgKey::NoteLinksFolder => "{0} · {1}",
        MsgKey::SidebarSummary => "{0} · {1}",
        MsgKey::GraphLinks => "{0} · {1}",
        MsgKey::RecoveredCopies => "{0} e {1}",

        MsgKey::MoreEdits => "+{0} mais",
        MsgKey::Deleted => "Excluído: {0}",
        MsgKey::ScanMs => "verificação {0} ms",
        MsgKey::RestoreTo => "Restaurar em {0}",
        MsgKey::FilterByTag => "Filtrar por #{0}",
        MsgKey::LinkImpactOne => "{0} nota vincula {1}",
        MsgKey::LinkImpactOther => "{0} notas vinculam {1}",

        MsgKey::CouldNotSave => "Não foi possível salvar: {0}",
        MsgKey::CouldNotOpenVault => "Não foi possível abrir o cofre: {0}",
        MsgKey::CouldNotReadVault => "Não foi possível ler o cofre: {0}",
        MsgKey::CouldNotReadNoteState => "Não foi possível ler o estado da nota: {0}",
        MsgKey::CouldNotCreateVault => "Não foi possível criar o cofre: {0}",
        MsgKey::CouldNotPersistKanbanRecovery => {
            "Não foi possível salvar a cópia de recuperação do kanban: {0}"
        }
        MsgKey::CouldNotPersistRecovery => "Não foi possível salvar a cópia de recuperação: {0}",
        MsgKey::CouldNotPersistBoardRecovery => {
            "Não foi possível salvar a cópia de recuperação do quadro: {0}"
        }
        MsgKey::CouldNotReadRecovery => "Não foi possível ler a cópia de recuperação: {0}",
        MsgKey::CouldNotRecreateNoteFolder => "Não foi possível recriar a pasta de notas: {0}",
        MsgKey::CouldNotRecreateNote => "Não foi possível recriar a nota: {0}",
        MsgKey::CouldNotReloadNote => "Não foi possível recarregar a nota: {0}",
        MsgKey::CouldNotSaveCopy => "Não foi possível salvar uma cópia: {0}",
        MsgKey::CouldNotRenameNote => "Não foi possível renomear a nota: {0}",
        MsgKey::CouldNotRenameFolder => "Não foi possível renomear a pasta: {0}",
        MsgKey::CouldNotDeleteNote => "Não foi possível excluir a nota: {0}",
        MsgKey::CouldNotDeleteFolder => "Não foi possível excluir a pasta: {0}",
        MsgKey::CouldNotEmptyTrash => "Não foi possível esvaziar a lixeira: {0}",
        MsgKey::CouldNotMoveNote => "Não foi possível mover a nota: {0}",
        MsgKey::CouldNotCreateFolder => "Não foi possível criar a pasta: {0}",
        MsgKey::CouldNotCreateNote => "Não foi possível criar uma nota: {0}",
        MsgKey::CouldNotAddNoteTemplate => "Não foi possível adicionar o modelo de nota: {0}",
        MsgKey::CouldNotBeRead => "não foi possível ler o {0}",
        MsgKey::CouldNotRestore => "Não foi possível restaurar {0}: {1}",
        MsgKey::CouldNotOpenFile => "Não foi possível abrir {0}: {1}. {2}",
        MsgKey::CouldNotRevealFile => "Não foi possível mostrar {0}: {1}. {2}",
        MsgKey::CouldNotImportInto => "Não foi possível importar em {0}: {1}",
        MsgKey::RefusingUnsafeRecoveryPath => "Recusando o caminho de recuperação inseguro {0}",
        MsgKey::BoardQuitRefused => {
            "O quadro não foi salvo e nenhuma cópia de recuperação pôde ser gravada: {0}. Use Salvar cópia ou corrija a pasta e saia novamente."
        }
        MsgKey::ScanCancelled => "Verificação cancelada após {0} de {1} notas",

        MsgKey::ImportRefused => "Importação recusada: {0}",
        MsgKey::ImportPartlyRefused => "Importação parcial: {0} de {1}; recusados: {2}",
        MsgKey::CannotImport => {
            "Não é possível importar {0}: arraste Markdown, imagens, PDF ou documentos de escritório"
        }
        MsgKey::ImportProgress => "Importando {0}…",
        MsgKey::ImportDocumentNotice => "Importado {0}",
        MsgKey::ImportUnsupported => {
            "Não foi possível importar {0}: este tipo de arquivo não pode ser convertido."
        }
        MsgKey::ImportNeedsOcrOne => {
            "Não foi possível importar {0}: a página {1} de {2} é uma imagem digitalizada e precisa de OCR, que o Muninn não faz. Exporte primeiro uma versão em texto."
        }
        MsgKey::ImportNeedsOcrOther => {
            "Não foi possível importar {0}: as páginas {1} de {2} são imagens digitalizadas e precisam de OCR, que o Muninn não faz. Exporte primeiro uma versão em texto."
        }
        MsgKey::ImportEncrypted => {
            "Não foi possível importar {0}: o documento está criptografado. Remova a senha e importe-o novamente."
        }
        MsgKey::ImportMalformed => {
            "Não foi possível importar {0}: o arquivo está danificado ou não é um documento legível."
        }
        MsgKey::ImportResourceLimit => {
            "Não foi possível importar {0}: o documento é complexo demais para converter."
        }
        MsgKey::ImportMissingPart => {
            "Não foi possível importar {0}: falta ao documento uma parte necessária."
        }
        MsgKey::ImportIo => "Não foi possível importar {0}: não foi possível ler o arquivo.",
        MsgKey::ImportFailed => {
            "Não foi possível importar {0}: não foi possível converter o documento."
        }
        MsgKey::ImportTooLarge => {
            "Não foi possível importar {0}: o arquivo ocupa {1}, acima do limite de {2}."
        }
        MsgKey::ImportBodyTooLarge => {
            "Não foi possível importar {0}: a nota convertida ocupa {1}, acima do limite de {2}."
        }
        MsgKey::RefusedNotAFile => "{0}: não é um arquivo",
        MsgKey::RefusedUnsupportedType => "{0}: tipo de arquivo não suportado",
        MsgKey::RefusedAlreadyExists => "{0}: já existe nesta pasta",

        MsgKey::ConnectedAs => {
            "Conectado como {0}. O token está no armazenamento de credenciais do sistema operacional. Clique em Sincronizar agora quando quiser."
        }
        MsgKey::LastSync => "Última sincronização bem-sucedida: {0}.",
        MsgKey::SyncStatus => "Sincronização: {0}",
        MsgKey::SyncExcluded => "Caminhos excluídos pelo contrato de sincronização: {0}.",
        MsgKey::SyncConflicts => "{0} conflito(s): {1}. Nada foi sobrescrito.",
        MsgKey::SyncConflictCopies => {
            "Uma nota mudou em dois dispositivos; as duas versões foram mantidas: {0}."
        }

        MsgKey::BoardNameTaken => "Já existe um quadro chamado “{0}” nesta pasta",
        MsgKey::NoNoteNamed => "Não há nenhuma nota chamada “{0}” neste cofre",
        MsgKey::LinkedNoteHint => "Nota vinculada: {0} · ⌘Enter abre",
        MsgKey::BoardCanvas => "Tela do quadro, {0}, ferramenta {1}",
        MsgKey::BoardCanvasSelected => "Tela do quadro, {0}, ferramenta {1}, {2} selecionado(s)",
        MsgKey::CardLabelEmpty => "{0}, cartão vazio",
        MsgKey::CardLabel => "{0}: {1}",

        MsgKey::ThemeLabel => "Tema: {0} — clique para alternar (Sistema, Escuro, Claro)",
        MsgKey::AutosaveHelp => {
            "O salvamento automático grava {0} s depois que você para de digitar."
        }
        MsgKey::Version => "Versão {0}.",
        MsgKey::ShowWithChord => "Mostrar o Muninn com {0}",
        MsgKey::ShortcutFailed => "Não foi possível registrar {0}: {1}",
        MsgKey::ShortcutOn => "Ativado. {0} mostra o Muninn a partir de qualquer aplicativo.",

        MsgKey::RelativeToday => "hoje",
        MsgKey::RelativeTomorrow => "amanhã",
        MsgKey::RelativeYesterday => "ontem",
        MsgKey::RelativeInDays => "em {0} dias",
        MsgKey::RelativeDaysAgo => "há {0} dias",
        MsgKey::DayLabel => "Dia {0}",
        MsgKey::RelativeDate => "{0} · {1}",

        MsgKey::NewNoteIn => "Nova nota em {0}",
        MsgKey::NewFolderIn => "Nova pasta em {0}",
        MsgKey::WelcomeVaultCreated => "Novo cofre criado em {0} — crie sua primeira nota",

        MsgKey::ScanProgress => "Verificando {0}/{1} notas — Esc para cancelar",
        MsgKey::FocusHelp => "Ocultar a interface e manter a linha do cursor centralizada ({0})",
        MsgKey::FocusExitHelp => "Voltar para a interface da nota ({0})",
        MsgKey::PaletteKanbanFolder => "Kanban · {0}",

        MsgKey::LinksNotUpdated => "{0}: os links não foram atualizados",
        MsgKey::RenamedLinksFailed => {
            "Renomeado, mas não foi possível reescrever {0} arquivo(s) com links de entrada: {1}"
        }
        MsgKey::MovedLinksFailed => {
            "Movido, mas não foi possível reescrever {0} arquivo(s) com links de entrada: {1}"
        }
        MsgKey::NoteNotInVault => "A nota {0} não está mais neste cofre",
        MsgKey::CannotOpenWithoutNote => "Não é possível abrir {0}: nenhuma nota está selecionada",
        MsgKey::AttachmentNotInVault => {
            "O anexo {0} não está neste cofre — arraste-o para cá ou corrija a incorporação"
        }
        MsgKey::BrokenLinksCount => "{0} não resolvidos",

        MsgKey::NotAFile => "{0} não é um arquivo",
        MsgKey::PathGone => "{0} não existe mais",
        MsgKey::TrayError => "Não foi possível adicionar o ícone da bandeja: {0}",
        MsgKey::LoginItemError => "Não foi possível alterar o item de inicialização: {0}",
        MsgKey::OutsideVault => "{0} está fora do cofre",
        MsgKey::NotABoardInVault => "{0} não é um quadro dentro do cofre",
        MsgKey::AlreadyExists => "{0} já existe",
        MsgKey::NotATrashPath => "{0} não é um caminho da lixeira",
        MsgKey::NoLongerInTrash => "{0} não está mais na lixeira",
        MsgKey::NotAFolder => "{0} não é uma pasta",
        MsgKey::BoardNameInvalid => "um nome de quadro {0}",
        MsgKey::BoardSceneKeyObject => "a cena {0} deve ser um objeto",
        MsgKey::BoardRenamedEmbedsFailed => {
            "Renomeado, mas não foi possível atualizar {0} nota(s) que o incorporam: {1}"
        }
        MsgKey::AutostartRegFailed => "Não foi possível executar o reg.exe: {0}",
        MsgKey::AutostartRegExit => "o reg.exe terminou com {0}",
        MsgKey::KanbanQuitRefused => {
            "O kanban não foi salvo e nenhuma cópia de recuperação pôde ser gravada: {0}. Use Salvar cópia ou corrija a pasta e saia novamente."
        }
        MsgKey::KanbanCouldNotSaveCopy => "não foi possível salvar uma cópia: {0}",
        MsgKey::CouldNotDo => "Não foi possível {0}: {1}",
        MsgKey::CouldNotCreateBoard => "Não foi possível criar um quadro: {0}",
        MsgKey::CouldNotCreateKanban => "Não foi possível criar um kanban: {0}",
        MsgKey::CouldNotConvertKanban => "Não foi possível converter em kanban: {0}",

        MsgKey::BoardKeptAsIs => "{0} · {1} mantido como está",
        MsgKey::BoardCouldNotRead => "Não foi possível ler: {0}",
        MsgKey::BoardNoteLinkMissing => "{0} (ausente)",
        MsgKey::UnsavedSaves => "Não salvo · {0} salvamentos",
        MsgKey::NotSaved => "Não salvo: {0}",
        MsgKey::LinkDescription => "Abrir {0}",
        MsgKey::NoteLabelRoot => "{0} (raiz)",
        MsgKey::GraphNodeParent => "{0} ({1})",
        MsgKey::GraphStats => "{0} notas · {1} arestas",

        MsgKey::KanbanFilterFrom => "de {0}",
        MsgKey::KanbanFilterTo => "até {0}",
        MsgKey::KanbanFilterRange => "{0} – {1}",
        MsgKey::LaneActions => "Ações da coluna: {0}",
        MsgKey::LaneControls => "Controles de {0}",
        MsgKey::CollapseLane => "Recolher a coluna: {0}",
        MsgKey::ExpandLane => "Expandir a coluna: {0}",
        MsgKey::LaneCards => "Cartões de {0}",
        MsgKey::AddCardToLane => "Adicionar cartão a {0}",
        MsgKey::LaneLabel => "{0}, {1} cartões",
        MsgKey::LaneOverLimit => ", acima do limite",
        MsgKey::LaneDone => ", coluna de concluídos",
        MsgKey::LaneRegionLabel => "Coluna {0}, {1} cartões",
        MsgKey::CollapsedLaneLabel => "{0}, {1} cartões, recolhida",
        MsgKey::CollapsedCards => "Recolhida · {0} cartões",
        MsgKey::DeleteLane => {
            "Excluir “{0}”{1}? Qualquer texto de nota contido nela permanece no arquivo."
        }
        MsgKey::DeleteLaneText => {
            "Excluir “{0}”{1}? Também será excluído o texto de nota que o quadro não mostra: {2}, a partir de “{3}”."
        }
        MsgKey::DeleteCardsOne => " e seu cartão",
        MsgKey::DeleteCardsOther => " e seus {0} cartões",
        MsgKey::CardActions => "Ações do cartão: {0}",
        MsgKey::CardActionsEmpty => "Ações do cartão: cartão vazio",
        MsgKey::CardActionsInLane => "Ações do cartão: cartão vazio em {0}",
        MsgKey::ChecklistProgress => "{0} de {1} concluídas",
        MsgKey::ChecklistLabel => "Lista de tarefas, {0} de {1} concluídas",
        MsgKey::CardLabelInLane => "{0} · {1}: {2}",

        MsgKey::DeviceCode => {
            "Digite o código {0} em {1}. Seu navegador está abrindo; o Muninn termina quando o GitHub aprova."
        }
        MsgKey::BrowserFailed => {
            "Não foi possível abrir um navegador. Abra este endereço você mesmo para continuar: {0}"
        }
        MsgKey::SignInFailed => "O login não terminou: {0} Nenhum token foi guardado.",
        MsgKey::TailnetHosting => "Hospedando em 127.0.0.1:{0}. Neste dispositivo, execute: {1}",
        MsgKey::TailnetHostingReady => {
            "Hospedando. Execute uma vez neste dispositivo: {0}. Use Copiar chave para dar a chave compartilhada aos seus outros dispositivos."
        }
        MsgKey::SharedKeyCopied => {
            "Chave compartilhada copiada. A área de transferência é limpa em {0} segundos; cole-a no dispositivo que está entrando."
        }
        MsgKey::SyncPreviousCredential => {
            "Não foi possível remover a credencial da conexão anterior: {0}"
        }
        MsgKey::SyncNothingSaved => "{0} Nada foi salvo.",
        MsgKey::SyncKeyRotationFailed => "{0} A chave antiga ainda funciona.",
        MsgKey::SyncNowAfterError => "{0} Conecte novamente antes de Sincronizar agora.",
        MsgKey::SyncOffline => "Sem conexão: {0}",
        MsgKey::SyncRateLimitedAfter => "Muitas solicitações; tente novamente em {0} ms",
        MsgKey::SyncProviderError => "Erro do provedor {0}: {1}",
        MsgKey::SyncAuthenticationNeeded => "Autenticação necessária: {0}",
        MsgKey::SyncPermissionDenied => "Permissão negada: {0}",
        MsgKey::SyncOauthScopeRejected => "Escopo OAuth rejeitado: {0}",
        MsgKey::SyncRemoteScopeNotFound => "Escopo remoto não encontrado: {0}",
        MsgKey::SyncRemoteChanged => "O remoto mudou: {0}",
        MsgKey::SyncOutcomeUnknown => {
            "Resultado desconhecido; reconcilie antes de tentar novamente: {0}"
        }
        MsgKey::SyncListingIncomplete => {
            "A listagem remota está incompleta; não será tratada como exclusão: {0}"
        }
        MsgKey::SyncUnsupported => "{0} não é compatível: {1}",
        MsgKey::SyncLimitExceeded => "Limite de sincronização excedido: {0}",
        MsgKey::SyncCredentialsUnavailable => "Credenciais indisponíveis: {0}",
        MsgKey::SyncDirtyBuffer => {
            "Salve ou descarte as edições não salvas em {0} antes que a sincronização possa alterá-la"
        }
        MsgKey::SyncLocalChanged => {
            "{0} mudou durante a sincronização; execute Sincronizar agora novamente para replanejar"
        }
        MsgKey::SyncNotConfigured => "A sincronização não está configurada: {0}",
        MsgKey::SyncFileError => "Erro de arquivo: {0}",

        MsgKey::RecoveredFor => "Edições não salvas recuperadas de {0}",
        MsgKey::ImportedFilesOne => "{0} importado",
        MsgKey::ImportedFilesOther => "{0} importados",
        MsgKey::ImportJoin => " · {0}",
        MsgKey::ImportAttachmentsNote => " · arquivos que não são Markdown continuam como anexos",
    }
}
