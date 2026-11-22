//! The `pt-BR` catalog.
//!
//! Brazilian Portuguese, neutral software register. This exhaustive `match`
//! mirrors [`crate::en_us`] arm for arm and in the same order, so a key added
//! to the catalog fails `cargo check` here until it has Portuguese text.
//! Identifiers, keycaps, product names and on-disk tokens stay as written; see
//! `docs/i18n.md` §5.8.
//!
//! The vocabulary is pt-BR, not pt-PT: `tela`, `arquivo`, `usuário`,
//! `configurações`, `salvar`, `excluir`, `senha`, `celular`. `arquivo` means
//! "file"; the archive feature is `arquivar`/`arquivados`, never `arquivo`.
//!
//! The copy is reviewed by a native or fluent speaker before release. The
//! choices that need care are recorded in `tests/portuguese.rs` (the invariant
//! allowlist) and in the pull request's terminology notes.

use crate::key::Key;

/// The Brazilian Portuguese text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping every catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Adicionar cartão",
        Key::Archive => "Arquivar",
        Key::Today => "Hoje",
        Key::OverLimit => "Acima do limite",
        Key::KeyboardShortcuts => "Atalhos de teclado",
        Key::Close => "Fechar",
        Key::Save => "Salvar",
        Key::Cancel => "Cancelar",
        Key::Clear => "Limpar",
        Key::LinkedNote => "Nota vinculada",
        Key::NoBoards => "Ainda não há quadros neste cofre",
        Key::HelpLastLogLines => "últimas linhas do registro:",
        Key::HelpGroupNavigation => "Navegação",
        Key::HelpNavFocusSearch => "Focar a busca",
        Key::HelpNavMoveHighlightedResult => "Mover o resultado destacado",
        Key::HelpNavOpenHighlightedResult => "Abrir o resultado destacado",
        Key::HelpNavBackInNoteHistory => "Voltar no histórico de notas",
        Key::HelpNavForwardInNoteHistory => "Avançar no histórico de notas",
        Key::HelpNavWriteReadGraphBoardKanban => "Escrita / Leitura / Gráfico / Quadro / Kanban",
        Key::HelpNavFocusMode => "Modo de foco: ocultar a interface e centralizar o cursor",
        Key::HelpNavCloseOutermostLayer => "Fechar a camada mais externa",
        Key::HelpNavOpenCommandPalette => "Abrir a paleta de comandos",
        Key::HelpGroupEdit => "Edição",
        Key::HelpEditFindInOpenNote => "Buscar na nota aberta",
        Key::HelpEditFindInBoard => "Buscar no quadro",
        Key::HelpEditHideCardsByTextTagDateOrState => {
            "Ocultar cartões por texto, etiqueta, data ou estado"
        }
        Key::HelpEditPerBoardOverrides => "Substituições por quadro, uma chave de cada vez",
        Key::HelpEditInsertWikilink => "Inserir um link wiki",
        Key::HelpEditNewNote => "Nova nota",
        Key::HelpEditImportDocument => "Importar um documento como nota",
        Key::HelpEditSaveNow => "Salvar agora (o salvamento automático está ativo)",
        Key::HelpEditToggleWriteRead => "Alternar escrita / leitura",
        Key::HelpEditWikilinkCompletion => "Autocompletar de links wiki",
        Key::HelpEditVaultTagCompletion => "Autocompletar de etiquetas do cofre",
        Key::HelpEditIndentFourSpaces => "Recuar quatro espaços",
        Key::HelpEditPasteRichText => {
            "Texto rico vira Markdown; uma URL sobre uma seleção vira link"
        }
        Key::HelpEditRenameSelectedNote => "Renomear a nota selecionada (no Quadro: o quadro)",
        Key::HelpEditDeleteSelectedNote => "Excluir a nota selecionada",
        Key::HelpGroupKanban => "Kanban",
        Key::HelpKanbanMoveThroughControls => "Navegar pelos controles do quadro",
        Key::HelpKanbanMoveBetweenLaneCards => "Navegar entre os cartões da coluna em foco",
        Key::HelpKanbanMoveSelectionAcrossLanes => "Mover a seleção entre as colunas",
        Key::HelpKanbanToggleCardDone => "Alternar a marcação de concluído do cartão selecionado",
        Key::HelpKanbanMoveSelectedCard => "Mover o cartão selecionado",
        Key::HelpKanbanMoveCardToEdgeOrLane => "Mover para cima, para baixo ou para outra coluna",
        Key::HelpKanbanOpenCardMenu => "Abrir o menu do cartão em foco",
        Key::HelpKanbanEditCard => "Editar o cartão",
        Key::HelpKanbanCloseMenuOrConfirmation => {
            "Fechar o menu, a confirmação ou a barra de conflito"
        }
        Key::HelpGroupGraph => "Gráfico",
        Key::HelpGraphToggleGraph => "Alternar gráfico",
        Key::HelpGraphToggleGlobalLocal => "Alternar global / local",
        Key::HelpGraphMoveFocusRing => "Mover o anel de foco",
        Key::HelpGraphOpenFocusedNote => "Abrir a nota em foco",
        Key::HelpGraphZoomInOut => "Aproximar / afastar",
        Key::HelpGraphFitVisibleNodes => "Ajustar os nós visíveis",
        Key::HelpGraphToggleIsolate => "Alternar isolamento",
        Key::HelpGraphLeaveGraph => "Sair do gráfico",
        Key::HelpGroupBoard => "Quadro",
        Key::HelpBoardPanWithDrag => "Deslocar o quadro (ou arrastar com a mão)",
        Key::HelpBoardZoomAtPointer => "Aproximar ou afastar no ponteiro",
        Key::HelpBoardZoomAtCanvasCentre => "Aproximar / afastar no centro do quadro",
        Key::HelpBoardPanTheBoard => "Deslocar o quadro",
        Key::HelpBoardFitTheDrawing => "Ajustar o desenho",
        Key::HelpBoardShapeTools => {
            "Ferramenta de retângulo, losango, elipse, seta, linha ou traço livre"
        }
        Key::HelpBoardDrawShape => "Desenhar essa forma; a ferramenta continua selecionada",
        Key::HelpBoardPlaceText => "Posicionar texto e digitá-lo no local com um clique",
        Key::HelpBoardEditTextLabel => "Escrever ou editar o rótulo de texto",
        Key::HelpBoardDropTheTool => "Soltar a ferramenta (voltar a selecionar)",
        Key::HelpBoardSelectElementOrToggle => {
            "Selecionar um elemento ou adicioná-lo ou removê-lo da seleção"
        }
        Key::HelpBoardNudgeSelection => "Mover a seleção 1 unidade (Shift: 5)",
        Key::HelpBoardDeleteSelection => "Excluir a seleção",
        Key::HelpBoardSelectEveryElement => "Selecionar todos os elementos",
        Key::HelpBoardLinkToNote => {
            "Vincular os elementos selecionados a uma nota (vazio desfaz o vínculo)"
        }
        Key::HelpBoardOpenLinkedNote => "Abrir a nota vinculada do elemento selecionado",
        Key::HelpBoardOpenClickedLinkedNote => {
            "Abrir a nota vinculada do elemento clicado (um clique só seleciona)"
        }
        Key::HelpBoardUndoRedo => "Desfazer / refazer desenhos, movimentos e exclusões",
        Key::HelpBoardSaveDrawing => "Salvar o desenho do quadro",
        Key::HelpBoardEscapeClearsSelection => {
            "Limpar a seleção; senão, soltar a ferramenta; senão, sair do Quadro"
        }
        Key::HelpGroupSidebar => "Barra lateral",
        Key::HelpSidebarOpenThisHelp => "Abrir esta Ajuda",
        Key::HelpSidebarOpenHelpWhenNotTyping => "Abrir a Ajuda quando não estiver digitando",
        Key::HelpSidebarMoveBetweenPanes => "Navegar entre painéis",
        Key::HelpSidebarMoveThroughAList => "Percorrer uma lista",
        Key::HelpSidebarOpenFocusedRow => "Abrir a linha em foco",
        Key::HelpSidebarNoteAndFolderActions => "Ações de nota e de pasta",
        Key::HelpSidebarSettings => {
            "Configurações (cofres, tema, efeitos do gráfico, padrões do Kanban)"
        }
        Key::SettingsTabVaults => "Cofres",
        Key::SettingsTabEditor => "Editor",
        Key::SettingsTabAppearance => "Aparência",
        Key::SettingsTabGraph => "Gráfico",
        Key::SettingsTabSync => "Sincronização",
        Key::SettingsTabKanban => "Kanban",
        Key::SettingsVaultsOpenTitle => "Abrir um cofre",
        Key::SettingsVaultsOpenCaption => {
            "Cole o caminho de uma pasta ou crie um cofre novo em uma pasta nova."
        }
        Key::SettingsVaultsRecentTitle => "Cofres recentes",
        Key::SettingsVaultsRecentCaption => {
            "Cofres que você abriu recentemente, primeiro os mais recentes."
        }
        Key::SettingsVaultsRecentEmpty => "Ainda não há cofres recentes",
        Key::SettingsVaultsPathLabel => "Caminho da pasta do cofre",
        Key::SettingsVaultsPathHint => {
            "Cole o caminho de uma pasta; ~ expande para sua pasta pessoal."
        }
        Key::SettingsVaultsOpenAction => "Abrir cofre",
        Key::SettingsVaultsNewAction => "Novo cofre",
        Key::SettingsEditorAutosaveTitle => "Atraso do salvamento automático",
        Key::SettingsEditorAutosaveCaption => {
            "Segundos de inatividade antes de salvar a nota aberta."
        }
        Key::SettingsEditorTemplateTitle => "Modelo de nota padrão",
        Key::SettingsEditorTemplateCaption => {
            "Markdown inserido no topo de cada nota nova. Deixe vazio para um título simples."
        }
        Key::SettingsEditorTemplateLabel => "Modelo de nota padrão",
        Key::SettingsEditorTemplatePlaceholder => "Markdown inserido no topo de cada nota nova",
        Key::SettingsAppearanceThemeTitle => "Tema",
        Key::SettingsAppearanceThemeFollowing => "Seguindo o tema do sistema Omarchy",
        Key::SettingsAppearanceLanguageTitle => "Idioma",
        Key::SettingsAppearanceLanguageCaption => {
            "A opção Sistema segue o idioma do seu sistema operacional e recorre ao inglês quando nenhum pacote de idioma corresponde."
        }
        Key::SettingsAppearanceLanguageSystem => "Sistema",
        Key::SettingsAppearanceUnavailable => "Ainda não disponível no macOS.",
        Key::SettingsAppearanceStateOn => "Ativado.",
        Key::SettingsAppearanceStateOff => "Desativado.",
        Key::SettingsAppearanceTrayTitle => "Ícone da bandeja do sistema",
        Key::SettingsAppearanceTrayCaption => {
            "Um ícone na bandeja com Mostrar Muninn, Nova nota, Sincronizar agora, o status da sincronização e Sair."
        }
        Key::SettingsAppearanceTrayShow => "Mostrar o ícone da bandeja",
        Key::SettingsAppearanceTrayStatusOn => {
            "Ativado. O Linux o mostra onde um serviço de bandeja está em execução; o GNOME precisa de uma extensão AppIndicator."
        }
        Key::SettingsAppearanceCloseToTrayTitle => "Fechar para a bandeja",
        Key::SettingsAppearanceCloseToTrayNeedsTray => {
            "Precisa do ícone da bandeja. Fechar a janela encerra o Muninn."
        }
        Key::SettingsAppearanceCloseToTrayOff => "Desativado. Fechar a janela encerra o Muninn.",
        Key::SettingsAppearanceCloseToTrayOn => {
            "Ativado. Fechar a janela mantém o Muninn na bandeja; Sair do Muninn por lá o encerra. Se nenhum serviço de bandeja mostrar o ícone, deixe esta opção desativada."
        }
        Key::SettingsAppearanceCloseToTrayNoIcon => {
            "Ativado, mas não há ícone na bandeja agora, então fechar a janela encerra o Muninn."
        }
        Key::SettingsAppearanceStartupTitle => "Inicialização e atalho",
        Key::SettingsAppearanceLoginTitle => "Iniciar o Muninn ao entrar",
        Key::SettingsAppearanceLoginOn => "Ativado. O Muninn abre quando você entra no sistema.",
        Key::SettingsAppearanceShortcutRegistering => "Registrando…",
        Key::SettingsAppearanceUpdateTitle => "Atualizações",
        Key::SettingsAppearanceUpdateCaption => {
            "O Muninn consulta o GitHub para saber se há uma versão estável mais recente. Nenhuma nota ou dado do cofre é enviado."
        }
        Key::SettingsAppearanceUpdateOff => {
            "Desativado. O Muninn não faz solicitações de atualização."
        }
        Key::SettingsAppearanceUpdateOn => {
            "Ativado. O Muninn consulta o GitHub uma vez por dia em busca de versões estáveis."
        }
        Key::SettingsGraphReduceMotionTitle => "Reduzir movimento",
        Key::SettingsGraphReduceMotionCaption => {
            "Congela o layout do gráfico, pula os deslocamentos de câmera e deixa estáticos os efeitos de ponteiro, seleção e gráfico. A opção Sistema segue a configuração do seu sistema operacional."
        }
        Key::SettingsGraphReduceMotionEnv => {
            "Ativado agora: MUNINN_REDUCED_MOTION está definida, o que substitui esta opção."
        }
        Key::SettingsGraphReduceMotionSystemOn => {
            "Ativado agora, seguindo a configuração do sistema."
        }
        Key::SettingsGraphReduceMotionSystemOff => {
            "Desativado agora, seguindo a configuração do sistema."
        }
        Key::SettingsGraphReduceMotionOn => "Ativado agora.",
        Key::SettingsGraphReduceMotionOff => "Desativado agora.",
        Key::SettingsGraphEffectsTitle => "Efeitos do gráfico",
        Key::SettingsGraphEffectsCaption => {
            "A mesma família de efeitos desenha a borda da seleção e as linhas que ela toca: Clássico mantém as linhas simples de sempre, Elétrico cria arcos nas duas e Desativado desenha linhas simples sem camadas de efeito. Reduzir movimento congela a opção escolhida."
        }
        Key::SettingsGraphIntensityTitle => "Intensidade do efeito",
        Key::SettingsGraphIntensityCaption => {
            "Escala os arcos de Elétrico e a queima ao excluir. Equilibrado é o visual original. Clássico não desenha arcos e Desativado não desenha nada, então a escolha de arcos só aparece com Elétrico; a queima continua escalando nos dois casos. Reduzir movimento também congela o resultado."
        }
        Key::SyncHeadingProvider => "Provedor de sincronização",
        Key::SyncHeadingProviderCaption => {
            "Somente sincronização manual com Sincronizar agora. Um provedor para este cofre. Desconectar remove as credenciais, não as notas."
        }
        Key::SyncProviderGithubName => "GitHub",
        Key::SyncProviderGithubDescription => {
            "GitHub selecionado. Use um repositório e uma branch dedicados, não o repositório do código do Muninn."
        }
        Key::SyncProviderDriveName => "Google Drive",
        Key::SyncProviderDriveShortName => "Drive",
        Key::SyncProviderDriveDescription => {
            "Google Drive selecionado. A sincronização mantém um histórico de commits gerenciado pelo aplicativo nos dados privados do Muninn no seu Drive, não em arquivos Markdown editáveis."
        }
        Key::SyncProviderDriveCaption => {
            "Mantido nos dados privados do Muninn no seu Drive, não em uma pasta visível. Use o mesmo nome de armazenamento em cada dispositivo."
        }
        Key::SyncProviderTailscaleName => "Tailscale",
        Key::SyncProviderTailscaleDescription => {
            "Tailscale selecionado. Um dispositivo hospeda um armazenamento de commits; os outros se conectam a ele pelo nome MagicDNS na sua tailnet."
        }
        Key::SyncFieldGithubOwner => "Proprietário do GitHub",
        Key::SyncFieldRepository => "Repositório",
        Key::SyncFieldBranch => "Branch",
        Key::SyncFieldPrefix => "Prefixo",
        Key::SyncFieldGithubClientId => "ID de cliente do GitHub (opcional)",
        Key::SyncFieldStoreName => "Nome do armazenamento",
        Key::SyncFieldGoogleClientId => "ID de cliente do Google (opcional)",
        Key::SyncFieldLoopbackPort => "Porta de loopback",
        Key::SyncFieldAllowedLogins => {
            "Logins do Tailscale permitidos (opcional, separados por vírgulas)"
        }
        Key::SyncFieldMagicDnsName => "Nome MagicDNS do host (device.tailnet.ts.net)",
        Key::SyncFieldSharedKey => "Chave compartilhada do host",
        Key::SyncActionConnect => "Conectar",
        Key::SyncActionSyncNow => "Sincronizar agora",
        Key::SyncActionDisconnect => "Desconectar",
        Key::SyncTailnetRoleLabel => "Função do Tailscale",
        Key::SyncTailnetRoleHost => "Hospedar aqui",
        Key::SyncTailnetRoleJoin => "Conectar-se a um host",
        Key::SyncTailnetKeyLabel => "Chave compartilhada",
        Key::SyncTailnetKeyCopy => "Copiar chave",
        Key::SyncTailnetKeyShow => "Mostrar chave",
        Key::SyncTailnetKeyHide => "Ocultar chave",
        Key::SyncTailnetKeyRotate => "Rotacionar chave",
        Key::SyncStatusNotConnected => {
            "Sem conexão. Escolha um provedor e um remoto explícito e depois clique em Conectar."
        }
        Key::SyncStatusTailnetHost => {
            "Este dispositivo hospeda. O Muninn serve o armazenamento de commits em 127.0.0.1 enquanto está aberto; o tailscale serve o publica na sua tailnet."
        }
        Key::SyncStatusTailnetJoin => {
            "Conecte-se a um host. Digite o nome MagicDNS completo do host e cole a chave de Copiar chave no host."
        }
        Key::SyncStatusConnectedSharedKey => {
            "Conectado. A chave compartilhada está no armazenamento de credenciais do sistema operacional. Clique em Sincronizar agora quando quiser."
        }
        Key::SyncStatusConnectedToken => {
            "Conectado. O token está no armazenamento de credenciais do sistema operacional. Clique em Sincronizar agora quando quiser."
        }
        Key::SyncStatusAskingGithubCode => "Solicitando ao GitHub um código de login…",
        Key::SyncStatusOpeningGoogle => "Abrindo o login do Google no seu navegador…",
        Key::SyncStatusWaitingGoogle => {
            "Conclua o login no Google no seu navegador. O Muninn aguarda a resposta em 127.0.0.1."
        }
        Key::SyncStatusSignedInAfterDisconnect => {
            "O login terminou depois de Desconectar; o token dele foi removido."
        }
        Key::SyncStatusTailnetJoined => {
            "Conectado. A chave compartilhada está no armazenamento de credenciais do sistema operacional. Sincronizar agora alcança o host pela sua tailnet."
        }
        Key::SyncStatusSyncStarted => "Sincronização iniciada. A janela continua respondendo.",
        Key::SyncStatusCancelling => {
            "Cancelando. Uma alteração que o provedor já aceitou é mantida e reconciliada."
        }
        Key::SyncStatusNoteChangedWhileSyncing => {
            "A nota aberta mudou durante a sincronização; o texto dela não foi sobrescrito."
        }
        Key::SyncStatusNothingConnected => "Nada está conectado. As notas locais não mudaram.",
        Key::SyncStatusDisconnected => {
            "Desconectado. As credenciais desta vinculação foram removidas. As notas locais e remotas não foram excluídas."
        }
        Key::SyncStatusSaveBeforeSync => {
            "Salve ou resolva a nota aberta antes de Sincronizar agora. O texto não salvo não é enviado nem sobrescrito."
        }
        Key::SyncStatusAlreadyRunning => "Já existe uma sincronização em andamento.",
        Key::SyncStatusFinishSignIn => {
            "Conclua o login primeiro ou clique em Desconectar para cancelar."
        }
        Key::SyncStatusConnectProvider => {
            "Conecte um provedor para este cofre antes de Sincronizar agora."
        }
        Key::SyncStatusWaitForRun => {
            "Aguarde a sincronização ou o login em andamento terminar, ou clique em Desconectar para cancelá-lo."
        }
        Key::SyncStatusWaitToRotateKey => {
            "Aguarde a sincronização em andamento terminar antes de rotacionar a chave."
        }
        Key::SyncStatusKeyRotated => {
            "Chave rotacionada; a chave antiga não funciona mais. Em cada dispositivo conectado, clique em Desconectar e entre novamente com a chave nova (Copiar chave)."
        }
        Key::SyncStatusCurrentConnectionKept => "A conexão atual não foi alterada.",
        Key::SyncErrorVaultNotCanonical => {
            "Esta pasta de cofre não pode ser canonicalizada, então não pode ser vinculada."
        }
        Key::SyncErrorGithubScope => {
            "Digite o proprietário e o repositório do GitHub antes de conectar."
        }
        Key::SyncErrorLoopbackPort => {
            "Digite uma porta de loopback entre 1024 e 65535 para o host."
        }
        Key::SyncErrorPasteSharedKey => {
            "Cole a chave compartilhada mostrada no host. Ela é guardada no armazenamento de credenciais do sistema operacional, não nas configurações."
        }
        Key::SyncErrorNotTailnetHost => "Este cofre não hospeda um armazenamento do Tailscale.",
        Key::SyncErrorSharedKeyMissing => {
            "A chave compartilhada está faltando no armazenamento de credenciais."
        }
        Key::SyncErrorTailnetKeyMissing => {
            "A chave compartilhada do Tailscale está faltando no armazenamento de credenciais. Desconecte e conecte novamente."
        }
        Key::SyncErrorCancelled => "Sincronização cancelada",
        Key::SyncErrorTimedOutAfterSend => {
            "Tempo esgotado após o envio de uma alteração; reconcilie antes de tentar novamente"
        }
        Key::SyncErrorTimedOutBeforeResponse => "Tempo esgotado antes de o provedor responder",
        Key::SyncErrorRateLimited => "Muitas solicitações",
        Key::SyncErrorLeaseBusy => "Outra instância do Muninn está gravando neste cofre",
        Key::SyncErrorSettingsNotObject => {
            "As configurações de sincronização não eram um objeto, então a sincronização está desativada."
        }
        Key::SyncErrorSettingsVersion => {
            "A versão das configurações de sincronização não é compatível, então a sincronização está desativada."
        }
        Key::SyncErrorSettingsVaultList => {
            "A lista de cofres de sincronização estava faltando, então a sincronização está desativada."
        }
        Key::SyncErrorSettingsBindingInvalid => {
            "Uma vinculação de sincronização era inválida e não foi carregada. A sincronização está desativada até que ela seja substituída."
        }
        Key::KanbanUiNotInsideVault => "o kanban não está dentro do cofre aberto",
        Key::KanbanUiThis => "o kanban",
        Key::KanbanUiFieldRefused => {
            "Não é possível sair: não foi possível salvar o kanban. Corrija o que impediu o salvamento ou desfaça as alterações e saia novamente."
        }
        Key::ErrorSaveKanbanBeforeRestore => {
            "Salve ou descarte o kanban atual antes de restaurar outro"
        }
        Key::PaletteTitle => "Paleta de comandos",
        Key::PaletteEmpty => "Nenhuma nota ou ação corresponde",
        Key::PaletteFooter => "↑↓ mover · Enter abrir · Esc fechar",
        Key::PaletteActionOpenLocalGraph => "Abrir o gráfico local",
        Key::PaletteActionSwitchVault => "Trocar de cofre",
        Key::PaletteActionOpenSettings => "Abrir as configurações",
        Key::PaletteActionNewNote => "Nova nota",
        Key::PaletteActionTogglePreview => "Alternar escrita / leitura",
        Key::PaletteActionToggleGraph => "Alternar gráfico",
        Key::PaletteActionToggleBoard => "Alternar quadro",
        Key::PaletteActionToggleKanban => "Alternar Kanban",
        Key::PaletteActionToggleFocusMode => "Alternar o modo de foco",
        Key::PaletteActionNewKanban => "Novo Kanban",
        Key::PaletteActionConvertToKanban => "Converter uma nota vazia em Kanban",
        Key::PaletteActionImportDocument => "Importar documento…",
        Key::PaletteHintGraphLocal => "Gráfico · Local",
        Key::PaletteHintVaults => "Cofres",
        Key::PaletteHintHelp => "Ajuda",
        Key::PaletteHintKanban => "Kanban",
        Key::PaletteKeywordsOpenLocalGraph => "gráfico local foco vizinhos alcance",
        Key::PaletteKeywordsSwitchVault => "cofre abrir recente pasta trocar",
        Key::PaletteKeywordsOpenSettings => "configurações preferências opções ajustes ajuda",
        Key::PaletteKeywordsNewNote => "nova nota criar adicionar",
        Key::PaletteKeywordsTogglePreview => "escrita leitura visualização editor modo",
        Key::PaletteKeywordsToggleGraph => "gráfico global local modo",
        Key::PaletteKeywordsToggleBoard => "quadro lousa excalidraw desenho tela esboço modo",
        Key::PaletteKeywordsToggleKanban => "kanban colunas cartões tarefas modo",
        Key::PaletteKeywordsToggleFocusMode => {
            "foco zen escrever sem distrações ocultar interface escurecer"
        }
        Key::PaletteKeywordsNewKanban => "kanban criar adicionar colunas cartões tarefas",
        Key::PaletteKeywordsConvertToKanban => {
            "kanban vazia em branco converter colunas cartões tarefas"
        }
        Key::PaletteKeywordsImportDocument => {
            "importar documento docx doc word pdf office odt rtf epub csv"
        }
        Key::PaletteSectionRecent => "Recentes",
        Key::PaletteSectionNotes => "Notas",
        Key::PaletteSectionActions => "Ações",
        Key::PaletteSectionKanbans => "Kanbans",
        Key::BoardPageTitle => "Quadros",
        Key::BoardPageNew => "Novo quadro",
        Key::BoardPageEmptyTitle => "Ainda não há quadros",
        Key::BoardPageEmptyDetail => "Crie um com Novo quadro",
        Key::BoardPageEmptyHint => {
            "Um quadro é um arquivo Excalidraw no seu cofre. Crie um para começar a desenhar."
        }
        Key::BoardPageNoSelectionTitle => "Nenhum quadro selecionado",
        Key::BoardPageNoSelectionDetail => {
            "Escolha um quadro no seletor acima para ver o que ele contém"
        }
        Key::BoardPageTitleLabel => "Nome do quadro; clique duas vezes para renomear",
        Key::BoardPageUnreadableTitle => "Não foi possível ler este quadro",
        Key::BoardPageNoOpenTitle => "Nenhum quadro aberto",
        Key::BoardPageNoOpenDetail => {
            "Escolha um quadro no seletor acima ou crie um novo. Quadros são arquivos Excalidraw no seu cofre."
        }
        Key::BoardPageEmptyCanvas => "Ainda não há nada desenhado neste quadro",
        Key::BoardPageHint => {
            "Clique/arraste selecionar · Del excluir · ⌘Z desfazer · R D O A L P desenhar · H deslocar · ⌘L vincular nota · ⌘S salvar · 0 ajustar"
        }
        Key::BoardPagePickerLabel => "Escolher um quadro",
        Key::BoardPagePickerHelp => "Abrir a lista de quadros",
        Key::BoardPagePickerMenu => "Quadros",
        Key::BoardToolToolbar => "Ferramentas do quadro",
        Key::BoardToolHand => "Mão",
        Key::BoardToolHandHelp => "Deslocar o quadro (H)",
        Key::BoardToolSelect => "Selecionar",
        Key::BoardToolSelectHelp => "Selecionar e mover elementos (V)",
        Key::BoardToolRectangle => "Retângulo",
        Key::BoardToolRectangleHelp => "Desenhar um retângulo (R)",
        Key::BoardToolDiamond => "Losango",
        Key::BoardToolDiamondHelp => "Desenhar um losango (D)",
        Key::BoardToolEllipse => "Elipse",
        Key::BoardToolEllipseHelp => "Desenhar uma elipse (O)",
        Key::BoardToolArrow => "Seta",
        Key::BoardToolArrowHelp => "Desenhar uma seta (A)",
        Key::BoardToolLine => "Linha",
        Key::BoardToolLineHelp => "Desenhar uma linha (L)",
        Key::BoardToolFreedraw => "Traço livre",
        Key::BoardToolFreedrawHelp => "Desenhar à mão livre (P ou X)",
        Key::BoardToolText => "Texto",
        Key::BoardToolTextHelp => "Posicionar texto e depois digitar (T)",
        Key::BoardLinkTitle => "Vincular nota",
        Key::BoardLinkHelp => "Vincular os elementos selecionados a uma nota (⌘L)",
        Key::BoardLinkName => "Nome ou caminho da nota",
        Key::BoardLinkHint => "um nome ou caminho do cofre · vazio desfaz o vínculo",
        Key::BoardRenameName => "Nome do quadro",
        Key::BoardPaletteTitle => "Cores do quadro",
        Key::BoardPaletteHelp => "Cores de traço e preenchimento",
        Key::BoardPaletteOpen => "Paleta de cores",
        Key::BoardPaletteClose => "Fechar a paleta de cores",
        Key::BoardPaletteStroke => "Traço",
        Key::BoardPaletteFill => "Preenchimento",
        Key::BoardPaletteSwatchBlack => "preto",
        Key::BoardPaletteSwatchGrey => "cinza",
        Key::BoardPaletteSwatchRed => "vermelho",
        Key::BoardPaletteSwatchOrange => "laranja",
        Key::BoardPaletteSwatchYellow => "amarelo",
        Key::BoardPaletteSwatchGreen => "verde",
        Key::BoardPaletteSwatchBlue => "azul",
        Key::BoardPaletteSwatchPurple => "roxo",
        Key::BoardPaletteSwatchTransparent => "transparente",
        Key::GraphPageEmpty => "Não há notas para desenhar",
        Key::GraphPageFindNode => "Buscar um nó do gráfico",
        Key::GraphPageFindPlaceholder => "Buscar nó · Enter",
        Key::GraphPageSelected => "SELECIONADO",
        Key::GraphPageHint => {
            "Setas focar · Enter abrir · +/- aproximar/afastar · 0/F ajustar · I isolar · Arrastar deslocar · Rolar aproximar/afastar"
        }
        Key::GraphScopeGlobal => "Global",
        Key::GraphScopeLocal => "Local",
        Key::GraphActionFit => "Ajustar",
        Key::GraphActionReset => "Redefinir",
        Key::GraphActionIsolate => "Isolar",
        Key::GraphActionOpenNote => "Abrir nota",
        Key::KanbanSectionDatesTitle => "Datas e horas",
        Key::KanbanSectionDatesCaption => {
            "Como as datas e as horas são digitadas, escritas e exibidas."
        }
        Key::KanbanSectionTagsTitle => "Etiquetas",
        Key::KanbanSectionTagsCaption => {
            "Etiquetas dos cartões e a lista de cores de etiqueta preservada."
        }
        Key::KanbanSectionMetadataTitle => "Metadados da nota vinculada",
        Key::KanbanSectionMetadataCaption => {
            "Chaves exibidas no rodapé do cartão a partir da primeira nota vinculada."
        }
        Key::KanbanSectionArchiveTitle => "Arquivados",
        Key::KanbanSectionArchiveCaption => {
            "O que o arquivamento escreve e quantos cartões ficam arquivados."
        }
        Key::KanbanSectionBoardTitle => "Quadro e cartões",
        Key::KanbanSectionBoardCaption => {
            "Largura da coluna, edição de cartões e botões do cabeçalho."
        }
        Key::KanbanSectionAppTitle => "Aplicativo",
        Key::KanbanSectionAppCaption => {
            "Opções para todos os quadros; não há substituição por quadro."
        }
        Key::KanbanChoiceEnter => "Enter",
        Key::KanbanChoiceShiftEnter => "Shift+Enter",
        Key::KanbanChoiceStart => "Início",
        Key::KanbanChoiceEnd => "Fim",
        Key::KanbanChoicePrepend => "Inserir no início",
        Key::KanbanChoiceAppend => "Adicionar ao final",
        Key::KanbanUiDefaultsNote => {
            "O rodapé de configurações do próprio quadro tem prioridade; caso contrário, estes valores valem para todos os quadros."
        }
        Key::KanbanUiBoardSettings => "Configurações do quadro",
        Key::KanbanUiChangesWriteOnly => {
            "As alterações gravam apenas essa chave no rodapé de configurações desta nota e podem ser desfeitas."
        }
        Key::KanbanUiNoMetadataKeys => "Nenhuma chave de metadados",
        Key::KanbanUiNoTagColours => "Nenhuma cor de etiqueta",
        Key::KanbanUiKeptAsWritten => "Mantido como escrito. Visualizar nunca o reescreve.",
        Key::KanbanUiSourceBoardIgnored => "Valor do quadro ignorado",
        Key::KanbanUiSourceBoardOverride => "Substituição do quadro",
        Key::KanbanUiSourceAppIgnored => "Valor do aplicativo ignorado",
        Key::KanbanUiSourceAppDefault => "Padrão do aplicativo",
        Key::KanbanUiSourceDefault => "Padrão",
        Key::KanbanUiResetBuiltIn => "Redefinir para o padrão interno",
        Key::KanbanUiResetBoard => "Redefinir quadro",
        Key::KanbanUiResetAppDefault => "Redefinir padrão do aplicativo",
        Key::KanbanUiRemove => "Remover",
        Key::KanbanUiAdd => "Adicionar",
        Key::KanbanUiContainsMarkdown => "Contém Markdown",
        Key::KanbanUiNewMetadataKey => "Nova chave de metadados",
        Key::KanbanUiUnlimited => "Ilimitado",
        Key::KanbanPageTitle => "Kanban",
        Key::KanbanPageViewSwitcher => "Visão do Kanban",
        Key::KanbanPageFilter => "Filtrar",
        Key::KanbanPageClearFilters => "Limpar filtros",
        Key::KanbanPageOpenMarkdown => "Abrir como Markdown",
        Key::KanbanPageResolveConflict => "Resolver conflito",
        Key::KanbanPageEmptyTitle => "Nenhum kanban aberto",
        Key::KanbanPageEmptyDetail => {
            if cfg!(target_os = "macos") {
                "Selecione uma nota de kanban e pressione ⌘5 para vê-la como quadro."
            } else {
                "Selecione uma nota de kanban e pressione Ctrl+5 para vê-la como quadro."
            }
        }
        Key::KanbanPageInvalidTitle => "Esta nota não pode ser exibida como kanban",
        Key::KanbanPageInvalidDetail => {
            "O rodapé de configurações dela está incompleto ou não é JSON válido. O arquivo é deixado como está."
        }
        Key::KanbanLaneComplete => "Concluída",
        Key::KanbanLaneNew => "Nova coluna",
        Key::KanbanLaneAdd => "Adicionar coluna",
        Key::KanbanLaneTitle => "Título da coluna",
        Key::KanbanLaneCardLimit => "Limite de cartões",
        Key::KanbanLaneNoLimit => "Sem limite",
        Key::KanbanCardText => "Texto do cartão",
        Key::KanbanCardOpenLink => "Abrir link",
        Key::KanbanViewBoard => "Quadro",
        Key::KanbanViewList => "Lista",
        Key::KanbanViewTable => "Tabela",
        Key::KanbanViewBoardTab => "Visão de quadro",
        Key::KanbanViewListTab => "Visão de lista",
        Key::KanbanViewTableTab => "Visão de tabela",
        Key::KanbanViewNoLanes => "Não há colunas neste quadro",
        Key::KanbanViewNoCards => "Não há cartões",
        Key::KanbanViewNoMatchingCards => "Nenhum cartão correspondente",
        Key::KanbanViewCardsByLane => "Cartões por coluna",
        Key::KanbanViewColumns => "Coluna, Cartão, Data, Etiquetas, Progresso",
        Key::KanbanViewColumnLane => "Coluna",
        Key::KanbanViewColumnCard => "Cartão",
        Key::KanbanViewColumnDate => "Data",
        Key::KanbanViewColumnTags => "Etiquetas",
        Key::KanbanViewColumnProgress => "Progresso",
        Key::KanbanFilterFind => "Buscar no quadro",
        Key::KanbanFilterTitle => "Filtrar cartões",
        Key::KanbanFilterText => "Texto",
        Key::KanbanFilterDate => "Data",
        Key::KanbanFilterAnyDate => "Qualquer data",
        Key::KanbanFilterOverdue => "Atrasado",
        Key::KanbanFilterStatus => "Status",
        Key::KanbanFilterAny => "Qualquer",
        Key::KanbanFilterOpen => "Aberto",
        Key::KanbanFilterDone => "Concluído",
        Key::KanbanFilterClearAll => "Limpar tudo",
        Key::KanbanFilterDateError => {
            "Um filtro de data usa YYYY-MM-DD; o intervalo não é aplicado."
        }
        Key::KanbanFilterPreviousMatch => "Correspondência anterior",
        Key::KanbanFilterNextMatch => "Próxima correspondência",
        Key::KanbanFilterCloseFind => "Fechar a busca",
        Key::KanbanArchiveEmpty => "Nenhum cartão arquivado.",
        Key::KanbanArchiveDelete => "Excluir",
        Key::KanbanArchiveRestore => "Restaurar",
        Key::KanbanArchiveRestoreTo => "Restaurar em…",
        Key::KanbanPickerAddDate => "Adicionar data",
        Key::KanbanPickerAddTime => "Adicionar hora",
        Key::KanbanPickerAddADate => "Adicionar uma data",
        Key::KanbanPickerAddATime => "Adicionar uma hora",
        Key::KanbanPickerHour => "Hora",
        Key::KanbanPickerMinute => "Minuto",
        Key::KanbanPickerNow => "Agora",
        Key::KanbanPickerPreviousMonth => "Mês anterior",
        Key::KanbanPickerNextMonth => "Próximo mês",
        Key::KanbanPickerDays => "Dias",
        Key::KanbanSettingDateTriggerLabel => "Gatilho de data",
        Key::KanbanSettingDateTriggerHelp => "Digitado antes de uma data, como em @{2026-11-12}.",
        Key::KanbanSettingTimeTriggerLabel => "Gatilho de hora",
        Key::KanbanSettingTimeTriggerHelp => "Digitado antes de uma hora, como em @@{14:30}.",
        Key::KanbanSettingDateFormatLabel => "Formato de data",
        Key::KanbanSettingDateFormatHelp => {
            "Escrito dentro de um token de data novo; YYYY, MM, DD, MMMM e Do são tokens."
        }
        Key::KanbanSettingTimeFormatLabel => "Formato de hora",
        Key::KanbanSettingTimeFormatHelp => {
            "Escrito dentro de um token de hora novo; HH, mm, h e A são tokens."
        }
        Key::KanbanSettingDateDisplayFormatLabel => "Formato de exibição de data",
        Key::KanbanSettingDateDisplayFormatHelp => "Como uma data é exibida no rodapé do cartão.",
        Key::KanbanSettingShowRelativeDateLabel => "Mostrar data relativa",
        Key::KanbanSettingShowRelativeDateHelp => "Adiciona hoje ou em N dias ao lado da data.",
        Key::KanbanSettingHideDateInTitleLabel => "Ocultar a data no título",
        Key::KanbanSettingHideDateInTitleHelp => {
            "Mantém os tokens de data e hora fora do texto principal do cartão."
        }
        Key::KanbanSettingLinkDateToDailyNoteLabel => "Vincular a data à nota diária",
        Key::KanbanSettingLinkDateToDailyNoteHelp => {
            "Faz a data exibida virar um link para a nota diária dela."
        }
        Key::KanbanSettingHideTagsInTitleLabel => "Ocultar as etiquetas no título",
        Key::KanbanSettingHideTagsInTitleHelp => {
            "Mantém as etiquetas (#) fora do texto principal do cartão."
        }
        Key::KanbanSettingShowTagsAtBottomLabel => "Mostrar as etiquetas no rodapé",
        Key::KanbanSettingShowTagsAtBottomHelp => "Desenha chips de etiqueta no rodapé do cartão.",
        Key::KanbanSettingTagColorsLabel => "Cores de etiqueta",
        Key::KanbanSettingTagColorsHelp => {
            "Mantido como escrito. O Muninn ainda não tem editor de cores."
        }
        Key::KanbanSettingLinkedPageMetadataLabel => "Metadados da nota vinculada",
        Key::KanbanSettingLinkedPageMetadataHelp => {
            "Chaves exibidas no rodapé do cartão a partir da primeira nota vinculada."
        }
        Key::KanbanSettingShowMissingMetadataLabel => "Mostrar metadados ausentes",
        Key::KanbanSettingShowMissingMetadataHelp => {
            "Desenha chave: — quando a nota vinculada ou a chave está ausente."
        }
        Key::KanbanSettingArchiveWithDateLabel => "Arquivar com data",
        Key::KanbanSettingArchiveWithDateHelp => {
            "Adiciona a data ao texto do cartão quando ele é arquivado."
        }
        Key::KanbanSettingArchiveDateFormatLabel => "Formato de data do arquivamento",
        Key::KanbanSettingArchiveDateFormatHelp => {
            "O formato de data usado ao arquivar; os mesmos tokens de Formato de data."
        }
        Key::KanbanSettingArchiveDatePositionLabel => "Posição da data do arquivamento",
        Key::KanbanSettingArchiveDatePositionHelp => "Início ou fim do texto do cartão arquivado.",
        Key::KanbanSettingArchiveDateSeparatorLabel => "Separador da data do arquivamento",
        Key::KanbanSettingArchiveDateSeparatorHelp => {
            "Texto entre o cartão e a data dele; um valor vazio os une."
        }
        Key::KanbanSettingMaxArchiveCardsLabel => "Máximo de cartões arquivados",
        Key::KanbanSettingMaxArchiveCardsHelp => {
            "Os cartões arquivados mais antigos acima desse número são descartados. 0 significa ilimitado."
        }
        Key::KanbanSettingLaneWidthLabel => "Largura da coluna",
        Key::KanbanSettingLaneWidthHelp => {
            "Largura da coluna em pontos; o quadro limita o que desenha a 120–720."
        }
        Key::KanbanSettingNewLineTriggerLabel => "Gatilho de nova linha",
        Key::KanbanSettingNewLineTriggerHelp => {
            "No editor de cartões, Enter confirma e Shift+Enter quebra a linha, ou o contrário."
        }
        Key::KanbanSettingNewCardInsertionLabel => "Posição dos cartões novos",
        Key::KanbanSettingNewCardInsertionHelp => {
            "Onde o botão + Adicionar cartão coloca o cartão novo."
        }
        Key::KanbanSettingShowCheckboxesLabel => "Mostrar caixas de seleção",
        Key::KanbanSettingShowCheckboxesHelp => "Desenha a caixa de concluído de cada cartão.",
        Key::KanbanSettingShowFilterButtonLabel => "Botão de filtro",
        Key::KanbanSettingShowFilterButtonHelp => "Mostra o botão Filtrar do cabeçalho.",
        Key::KanbanSettingShowArchiveButtonLabel => "Botão de arquivamento",
        Key::KanbanSettingShowArchiveButtonHelp => "Mostra o botão Arquivar do cabeçalho.",
        Key::KanbanSettingShowOpenMarkdownButtonLabel => "Botão Abrir como Markdown",
        Key::KanbanSettingShowOpenMarkdownButtonHelp => {
            "Mostra o botão Abrir como Markdown do cabeçalho."
        }
        Key::KanbanSettingNewNoteFolderLabel => "Pasta de notas novas",
        Key::KanbanSettingNewNoteFolderHelp => {
            "Pasta para Nova nota a partir do cartão; vazia usa a pasta de Nova nota."
        }
        Key::KanbanSettingNewNoteTemplateLabel => "Modelo de nota nova",
        Key::KanbanSettingNewNoteTemplateHelp => {
            "Um caminho de nota cujo texto inicia a nota nova; vazio usa o modelo de nota do aplicativo."
        }
        Key::KanbanSettingKanbanAutosaveLabel => "Salvamento automático das edições do kanban",
        Key::KanbanSettingKanbanAutosaveHelp => {
            "Salva as edições do kanban pendentes quando você para de editar. Desativado deixa o salvamento para ⌘S."
        }
        Key::StatusSaving => "Salvando…",
        Key::StatusSaved => "Salvo",
        Key::StatusAutosaveOn => "Salvamento automático ativo",
        Key::StatusRetry => "Tentar novamente",
        Key::StatusUndo => "Desfazer",
        Key::RecoveryRestore => "Restaurar",
        Key::RecoveryRestoreAll => "Restaurar tudo",
        Key::RecoveryDismiss => "Descartar",
        Key::RecoveryWriteEdits => "— Edições de escrita",
        Key::RecoveryKanbanEdits => "— Edições do Kanban",
        Key::UpdateAvailableTitle => "Atualização disponível",
        Key::UpdateReleaseNotes => "Notas da versão",
        Key::UpdateDismiss => "Descartar",
        Key::UpdateHintAppImage => "Baixe o novo AppImage na página da versão.",
        Key::UpdateHintAur => "Atualize com o seu assistente de AUR.",
        Key::UpdateHintDeb => "Baixe e instale o novo .deb.",
        Key::UpdateHintTarball => "Execute install.sh novamente ou baixe o novo arquivo tar.",
        Key::UpdateHintUnknown => "Veja a página da versão.",
        Key::UpdateOpenFailed => "Não foi possível abrir a página da versão no navegador.",
        Key::BrokenAllResolve => "Todos os links foram resolvidos",
        Key::BrokenTitle => "Links não resolvidos",
        Key::TopbarNoNoteSelected => "Nenhuma nota selecionada",
        Key::TopbarOpenAsKanban => "Abrir como kanban",
        Key::TrashTitle => "Lixeira",
        Key::TrashEmptyAction => "Esvaziar a lixeira",
        Key::TrashExplainer => {
            "As notas excluídas ficam aqui até serem desfeitas ou removidas permanentemente. No máximo 500 arquivos ou 100 MB são mantidos; a pasta com o carimbo de data mais antigo é podada primeiro."
        }
        Key::TrayShow => "Mostrar Muninn",
        Key::TrayNewNote => "Nova nota",
        Key::TraySyncNow => "Sincronizar agora",
        Key::TrayQuit => "Sair do Muninn",
        Key::TrayNotConnected => "Sem conexão.",
        Key::TrayTooltip => "Muninn",
        Key::ErrorEnterVaultPath => "Digite um caminho de pasta do cofre",
        Key::ErrorEnterNewVaultPath => "Digite um caminho de pasta para o cofre novo",
        Key::ErrorSaveNoteBeforeSwitch => "Salve ou resolva a nota atual antes de trocar de cofre",
        Key::ErrorSaveBoardBeforeSwitch => "Salve o quadro atual antes de trocar de cofre",
        Key::ErrorSaveKanbanBeforeSwitch => "Salve o kanban atual antes de trocar de cofre",
        Key::ErrorSaveNoteBeforeRestore => {
            "Salve ou descarte a nota atual antes de restaurar outra"
        }
        Key::ErrorSaveBoardBeforeRestore => {
            "Salve ou descarte o quadro atual antes de restaurar outro"
        }
        Key::ErrorSaveBoardBeforeRescan => {
            "Salve o quadro atual antes de verificar o cofre novamente"
        }
        Key::ErrorOpenerHintUnix => {
            "Instale xdg-utils (para xdg-open) ou abra o arquivo manualmente."
        }
        Key::ErrorNoFileOpener => {
            "Não há nenhum aplicativo documentado para abrir arquivos nesta plataforma; abra o arquivo manualmente"
        }
        Key::ErrorOpenerHintMacos => {
            "O comando `open` do sistema falhou; abra o arquivo pelo Finder."
        }
        Key::ErrorOpenerHintWindows => {
            "O aplicativo do sistema não conseguiu abrir o arquivo; abra-o pelo Explorador de Arquivos."
        }
        Key::ErrorOpenerHintOther => "Abra o arquivo manualmente.",
        Key::ErrorDocumentBoard => "quadro",
        Key::ErrorDocumentKanban => "kanban",
        Key::ImportDocument => "Importar documento…",
        Key::ImportDocumentHelp => "Importar um documento como nota nova",
        Key::ImportDialogTitle => "Importar documento",
        Key::ImportDialogFilterDocuments => "Documentos",
        Key::ErrorImportDialogUnavailable => {
            "Não há diálogo de arquivos neste ambiente; arraste o documento para a janela."
        }
        Key::StatusTrashEmptied => "Lixeira esvaziada",
        Key::StatusLinkCopied => "Link do cartão copiado",
        Key::ConflictExternalChange => "Alteração externa detectada",
        Key::ConflictFileDeleted => "arquivo excluído externamente",
        Key::ConflictChangedOnDisk => "Alterado no disco por outro aplicativo",
        Key::ConflictMissingOnDisk => "Removido ou renomeado no disco",
        Key::ConflictKeepMine => "Manter a minha",
        Key::ConflictLoadTheirs => "Carregar a versão externa",
        Key::ConflictSaveCopy => "Salvar cópia",
        Key::ConflictDiscard => "Descartar",
        Key::CommonAppName => "Muninn",
        Key::CommonDelete => "Excluir",
        Key::CommonEdit => "Editar",
        Key::CommonMove => "Mover",
        Key::CommonNewNote => "Nova nota",
        Key::CommonOpen => "Abrir",
        Key::CommonRename => "Renomear",
        Key::CommonResults => "Resultados",
        Key::CommonVaultRoot => "Raiz do cofre",
        Key::CommonDismiss => "Descartar",
        Key::CommonOn => "Ativado",
        Key::CommonOff => "Desativado",
        Key::SidebarNoNotesTitle => "Ainda não há notas",
        Key::SidebarNoNotesHint => "Adicione arquivos .md aqui ou crie um abaixo.",
        Key::SidebarNoMatchTitle => "Nenhuma nota corresponde",
        Key::SidebarNoMatchHint => "Tente outra busca.",
        Key::SidebarSearch => "Buscar notas",
        Key::SidebarLinksOk => "links ok",
        Key::SidebarKanbans => "KANBANS",
        Key::SidebarBoards => "QUADROS",
        Key::SidebarTheVaultRoot => "a raiz do cofre",
        Key::SettingsTitle => "Configurações",
        Key::TopbarSettingsHelp => "Configurações (Ctrl+,)",
        Key::TopbarHideSidebar => "Ocultar a barra lateral",
        Key::TopbarShowSidebar => "Mostrar a barra lateral",
        Key::SettingsChangesSave => "As alterações são salvas automaticamente.",
        Key::SettingsClose => "Fechar as configurações (Esc)",
        Key::SettingsSection => "Seção de configurações",
        Key::SettingsAutosave => "Salvamento automático",
        Key::SettingsDiagnostics => "Diagnóstico",
        Key::SettingsCopyDiagnostics => "Copiar o diagnóstico",
        Key::SettingsReportIssue => "Relatar um problema no GitHub",
        Key::SettingsThemeOmarchy => "Omarchy",
        Key::SettingsThemeSystem => "Sistema",
        Key::SettingsThemeDark => "Escuro",
        Key::SettingsThemeLight => "Claro",
        Key::SettingsMotionSystem => "Sistema",
        Key::SettingsMotionOn => "Ativado",
        Key::SettingsMotionOff => "Desativado",
        Key::SettingsEffectsClassic => "Clássico",
        Key::SettingsEffectsElectric => "Elétrico",
        Key::SettingsEffectsOff => "Desativado / Mínimo",
        Key::SettingsIntensitySubtle => "Sutil",
        Key::SettingsIntensityBalanced => "Equilibrado",
        Key::SettingsIntensityIntense => "Intenso",
        Key::WelcomeGraphLabel => "Abrir o gráfico de links",
        Key::WelcomeGraphHelp => "Veja como as notas se conectam por meio de [[links]]",
        Key::WelcomeGraphPill => "[[uma nota]] → gráfico",
        Key::PreviewEmptyNote => "Esta nota está vazia. Escreva algo no editor.",
        Key::PreviewStartWriting => "Crie uma nota para começar a escrever.",
        Key::PreviewEditor => "Editor de notas",
        Key::PreviewEditorPlaceholder => "Comece a escrever. Digite [[ para vincular outra nota.",
        Key::PreviewCompletions => "Autocompletar",
        Key::PreviewAttachments => "ANEXOS",
        Key::PreviewAttachmentsNote => "Os arquivos ficam fora do índice de notas",
        Key::PreviewOpen => "Abrir",
        Key::PreviewOpenBoard => "Abrir quadro",
        Key::PreviewMissingFile => "arquivo ausente",
        Key::PreviewBoardKind => "quadro",
        Key::PreviewAttachmentKind => "anexo",
        Key::PreviewMissing => "ausente",
        Key::PreviewNoOutgoing => "Nenhum link de saída",
        Key::PreviewNoBacklinks => "Ainda não há backlinks",
        Key::PreviewLinks => "LINKS",
        Key::PreviewOutgoing => "LINKS DE SAÍDA",
        Key::PreviewBacklinks => "BACKLINKS",
        Key::PreviewGraph => "GRÁFICO",
        Key::PreviewGraphHint => "Veja esta nota e tudo o que ela toca.",
        Key::PreviewFind => "Buscar na nota",
        Key::StartupScanning => "Verificando seu cofre",
        Key::StartupScanCancelled => "Verificação cancelada",
        Key::StartupCouldNotOpen => "Não foi possível abrir o cofre",
        Key::StartupRetryHint => {
            "Tentar novamente verifica o cofre outra vez, ou escolha outra pasta em Configurações (⌘,)."
        }
        Key::StartupPickFolderHint => {
            "Escolha outra pasta em Configurações (⌘,), ou passe --vault / defina MUNINN_VAULT."
        }
        Key::ModeWrite => "Escrita",
        Key::ModeRead => "Leitura",
        Key::ModeGraph => "Gráfico",
        Key::ModeBoard => "Quadro",
        Key::ModeKanban => "Kanban",
        Key::FocusExit => "Sair do modo de foco",
        Key::FocusEnter => "Modo de foco",
        Key::MenuOpenExternally => "Abrir externamente",
        Key::MenuReveal => "Mostrar no gerenciador de arquivos",
        Key::MenuMoveTo => "Mover para…",
        Key::MenuNewNoteHere => "Nova nota aqui",
        Key::MenuNewFolder => "Nova pasta…",
        Key::MenuInsertBefore => "Inserir cartão antes",
        Key::MenuInsertAfter => "Inserir cartão depois",
        Key::MenuDuplicate => "Duplicar",
        Key::MenuMarkOpen => "Marcar como aberto",
        Key::MenuMarkDone => "Marcar como concluído",
        Key::MenuChangeDate => "Alterar data…",
        Key::MenuAddDate => "Adicionar uma data…",
        Key::MenuChangeTime => "Alterar hora…",
        Key::MenuAddTime => "Adicionar uma hora…",
        Key::MenuSplitCard => "Dividir cartão",
        Key::MenuCopyLink => "Copiar link do cartão",
        Key::MenuNewNoteFromCard => "Nova nota a partir do cartão",
        Key::MenuMarkAllDone => "Marcar todos os cartões como concluídos",
        Key::MenuNoteActions => "Ações da nota",
        Key::MenuFolderActions => "Ações da pasta",
        Key::MenuBoardActions => "Ações do quadro",
        Key::MenuCardActions => "Ações do cartão",
        Key::MenuLaneActions => "Ações da coluna",
        Key::MenuArchiveActions => "Ações dos arquivados",
        Key::MenuMoveToLane => "Mover para a coluna",
        Key::LinkImpactHere => "aqui",
        Key::LinkImpactIntoFolder => "a esta pasta",
        Key::LinkThisNote => "Esta nota",
        Key::LinkNameFallback => "Link",
        Key::RenameRewriteLinks => "Reescrever os links para o novo nome",
        Key::RenameLeaveDangling => "Deixar os links sem resolução",
        Key::DeleteNoteQuestion => "Excluir esta nota?",
        Key::DeleteFolderQuestion => "Excluir a pasta? (somente se vazia)",
        Key::NewNoteName => "Nome da nota nova",
        Key::NoteNamePlaceholder => "Nome da nota",
        Key::RenameNoteName => "Renomear nota",
        Key::FolderNamePlaceholder => "Nome da pasta",
        Key::NewFolderName => "Nome da pasta nova",
        Key::RenameFolderName => "Renomear pasta",
        Key::PaletteMissingNote => "Nota ausente",
        Key::PaletteKanbanRoot => "Kanban · Raiz do cofre",
        Key::BoardRenameSaveFirst => "Salve o quadro primeiro: não foi possível salvar",
        Key::KanbanFilterToday => "hoje",
        Key::KanbanFilterOverdueWord => "atrasado",
        Key::KanbanFilterOpenWord => "aberto",
        Key::KanbanFilterDoneWord => "concluído",
        Key::KanbanDailyNoteLink => "Abrir a nota diária da data",
        Key::KanbanLaneMoveUp => "Mover para cima",
        Key::KanbanLaneMoveTop => "Mover para o topo",
        Key::KanbanLaneMoveDown => "Mover para baixo",
        Key::KanbanLaneMoveBottom => "Mover para o final",
        Key::KanbanLaneMovePreviousList => "Mover para a coluna anterior",
        Key::KanbanLaneMoveNextList => "Mover para a próxima coluna",
        Key::KanbanLaneMoveToList => "Mover para uma coluna…",
        Key::KanbanLaneInsertBefore => "Inserir coluna antes",
        Key::KanbanLaneInsertAfter => "Inserir coluna depois",
        Key::KanbanLaneMoveLeft => "Mover a coluna para a esquerda",
        Key::KanbanLaneMoveRight => "Mover a coluna para a direita",
        Key::KanbanLaneRename => "Renomear coluna",
        Key::KanbanLaneLimit => "Limite de cartões…",
        Key::KanbanLaneMarkNotDone => "Marcar como coluna não concluída",
        Key::KanbanLaneMarkDone => "Marcar como coluna concluída",
        Key::KanbanLaneSortText => "Classificar por texto",
        Key::KanbanLaneSortDate => "Classificar por data",
        Key::KanbanLaneSortTags => "Classificar por etiquetas",
        Key::KanbanLaneDelete => "Excluir coluna",
        Key::KanbanLaneDeleteCards => "Excluir a coluna e os cartões dela",
        Key::KanbanLaneDeleteText => "Excluir a coluna e o texto de nota dela",
        Key::KanbanCardFallback => "Cartão",
        Key::KanbanCardMarkDone => "Marcar como concluído",
        Key::KanbanCardMarkOpen => "Marcar como não concluído",
        Key::KanbanCardChecklistItem => "Item da lista de tarefas",
        Key::KanbanEditAddCard => "adicionar o cartão",
        Key::KanbanEditEditCard => "editar o cartão",
        Key::KanbanEditDeleteCard => "excluir o cartão",
        Key::KanbanEditDuplicateCard => "duplicar o cartão",
        Key::KanbanEditSplitCard => "dividir o cartão",
        Key::KanbanEditChangeDoneMark => "alterar a marcação de concluído",
        Key::KanbanEditToggleChecklistItem => "alternar o item da lista de tarefas",
        Key::KanbanEditMarkCardsDone => "marcar os cartões como concluídos",
        Key::KanbanEditAddLane => "adicionar a coluna",
        Key::KanbanEditRenameLane => "renomear a coluna",
        Key::KanbanEditClearLimit => "limpar o limite",
        Key::KanbanEditSetLimit => "definir o limite",
        Key::KanbanEditDeleteLane => "excluir a coluna",
        Key::KanbanEditChangeDoneLane => "alterar a coluna de concluídos",
        Key::KanbanEditSortLane => "classificar a coluna",
        Key::KanbanEditCollapseLane => "recolher a coluna",
        Key::KanbanEditMoveCard => "mover o cartão",
        Key::KanbanEditChangeDate => "alterar a data",
        Key::KanbanEditChangeTime => "alterar a hora",
        Key::KanbanEditClearDate => "limpar a data",
        Key::KanbanEditClearTime => "limpar a hora",
        Key::KanbanEditChangeBoardSetting => "alterar a configuração do quadro",
        Key::KanbanEditResetBoardSetting => "redefinir a configuração do quadro",
        Key::KanbanEditCopyCardLink => "copiar o link do cartão",
        Key::KanbanEditMakeNoteFromCard => "criar uma nota a partir do cartão",
        Key::KanbanEditArchiveCard => "arquivar o cartão",
        Key::KanbanEditRestoreCard => "restaurar o cartão arquivado",
        Key::KanbanEditDeleteArchivedCard => "excluir o cartão arquivado",
        Key::KanbanStatusLaneRefused => "a coluna em edição foi recusada",
        Key::KanbanStatusCardRefused => "o cartão em edição foi recusado",
        Key::KanbanStatusNotInsideVault => "o kanban não está dentro do cofre aberto",
        Key::KanbanStatusNoteGone => "a nota não está mais no cofre",
        Key::KanbanStatusFileUnchecked => "não foi possível verificar o arquivo",
        Key::KanbanStatusFileRemoved => "o arquivo foi removido",
        Key::KanbanStatusFileChanged => "o arquivo mudou no disco",
        Key::KanbanTimeInvalid => {
            "Um horário precisa de uma hora entre 0 e 23 e um minuto entre 0 e 59"
        }
        Key::KanbanLaneLimitInvalid => "Um limite de cartões é um número inteiro",
        Key::VaultEmptyFolderName => "nome de pasta vazio",
        Key::VaultCannotRenameRoot => "não é possível renomear a raiz",
        Key::VaultCannotDeleteRoot => "não é possível excluir a raiz",
        Key::VaultDestinationExists => "o destino já existe",
        Key::BoardParseNotObject => "um quadro deve ser um objeto JSON",
        Key::BoardParseNotExcalidraw => {
            "não é uma cena do Excalidraw (o tipo não é \"excalidraw\")"
        }
        Key::BoardParseVersionNumber => "a versão da cena deve ser um número inteiro",
        Key::BoardParseSourceString => "a origem da cena deve ser uma string",
        Key::BoardParseElementsArray => "os elementos da cena devem ser um array",
        Key::BoardStatusUnreadable => "não foi possível ler o quadro, então ele não é gravado",
        Key::BoardStatusOtherVault => "o quadro pertence a outro cofre",
        Key::BoardStatusInsideVault => "o quadro não está dentro do cofre aberto",
        Key::KanbanEditMoveLane => "mover a coluna",
        Key::AutostartPathNotUtf8 => "o caminho do executável não é UTF-8",
        Key::AutostartPathLineBreak => "o caminho do executável contém uma quebra de linha",
        Key::NoteNoLongerIndexed => "a nota não está mais indexada",
        Key::TrashEmpty => "A lixeira está vazia",
        Key::CoreBoardNameEmpty => "não pode ficar vazio",
        Key::CoreBoardNameSeparators => "não pode conter / nem \\",
        Key::CoreBoardNameHidden => "não pode começar com um ponto",
        Key::CoreBoardNameForbidden => "não pode conter : * ? \" < > | nem caracteres de controle",
        Key::CoreBoardNameTrailingDot => "não pode terminar com ponto",
        Key::CoreBoardNameReserved => "não pode ser um nome de dispositivo reservado",
        Key::MenuLockNote => "Bloquear nota…",
        Key::MenuUnlockNote => "Desbloquear nota…",
        Key::LockNoteTitle => "Bloquear nota",
        Key::UnlockNoteTitle => "Desbloquear nota",
        Key::LockPassword => "Senha",
        Key::LockConfirmPassword => "Confirmar senha",
        Key::LockLostPasswordWarning => {
            "Não há redefinição de senha, chave de recuperação nem conta Muninn. Se você perder \
             esta senha, a nota será perdida. Backups e o histórico de sincronização anteriores \
             ainda podem conter o texto não criptografado."
        }
        Key::LockSubmit => "Bloquear",
        Key::UnlockSubmit => "Desbloquear",
        Key::LockPasswordMismatch => "As senhas não coincidem.",
        Key::LockPasswordEmpty => "Digite uma senha.",
        Key::LockedNoteHeading => "Esta nota está bloqueada",
        Key::LockedNoteBody => "Desbloqueie-a com a senha para lê-la ou editá-la.",
        Key::LockedNoteRecoveryOff => {
            "A recuperação de falhas fica desativada para uma nota bloqueada; edições não salvas \
             dependem do salvamento automático."
        }
        Key::LockedNoteBadge => "Nota bloqueada",
        Key::UnlockWrongPassword => "Essa senha não abriu a nota.",
        Key::UnlockUnsupportedVersion => {
            "Esta nota foi bloqueada por uma versão mais recente do Muninn."
        }
        Key::UnlockMalformed => "Esta nota bloqueada está danificada.",
        Key::UnlockWrongNote => "Esta nota bloqueada pertence a outro nome de arquivo.",
        Key::UnlockRollback => "Esta nota é mais antiga que a última versão vista pelo Muninn.",
        Key::UnlockFailed => "Não foi possível desbloquear a nota.",
        Key::LockRefusedSave => "Esta nota está bloqueada. Desbloqueie-a para manter suas edições.",
        Key::LockRefusedRename => "Desbloqueie esta nota antes de renomeá-la ou movê-la.",
        Key::LockRefusedFolderRename => {
            "Desbloqueie ou remova as notas bloqueadas desta pasta antes de renomeá-la."
        }
        Key::LockConflictSaveCopy => {
            "Não é possível salvar uma cópia de uma nota bloqueada. Desbloqueie-a primeiro."
        }
        Key::HelpGroupLock => "Notas bloqueadas",
        Key::HelpLockLock => "Bloquear nota: adicione uma senha; o arquivo se torna texto cifrado.",
        Key::HelpLockUnlock => "Desbloquear nota: digite a senha para esta sessão.",
        Key::HelpLockLost => {
            "Não há redefinição de senha nem chave de recuperação. Uma senha perdida perde a nota."
        }
        Key::HelpLockRecovery => {
            "A recuperação de falhas fica desativada para notas bloqueadas; o salvamento \
             automático continua gravando o envelope."
        }
        Key::HelpLockRename => "Desbloqueie uma nota bloqueada antes de renomeá-la ou movê-la.",
        Key::PaletteActionLockNote => "Bloquear nota",
        Key::PaletteActionUnlockNote => "Desbloquear nota",
        Key::PaletteKeywordsLockNote => "criptografar senha privado proteger",
        Key::PaletteKeywordsUnlockNote => "abrir descriptografar senha",
    }
}
