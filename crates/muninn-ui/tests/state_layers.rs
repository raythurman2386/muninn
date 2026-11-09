//! Explicit dismissal layers: Escape must peel exactly one layer per press,
//! in the documented stack order, and `Layer::dismiss_layer` must close it.
//!
//! The open_* helpers replace each other (opening Settings dismisses Help),
//! so stacking is done with direct field writes — the same fields the view
//! reads — which is how the flags could ever be true at once.

use muninn_core::testing::TempVault;
use muninn_ui::{
    ContextMenu, FolderPrompt, FolderRename, FolderRenameConfirm, Layer, MenuTarget, MoveConfirm,
    NamePrompt, RenameConfirm, RenamePrompt, State,
};

fn state() -> (TempVault, State) {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let state = State::with_state_home(
        vault.path().to_path_buf(),
        vault.path().with_extension("state"),
    );
    (vault, state)
}

fn set_layer(state: &mut State, layer: Layer, on: bool) {
    match layer {
        Layer::Palette => {
            if on {
                state.open_palette();
            } else {
                state.close_palette();
            }
        }
        Layer::Settings => state.settings_modal = on,
        Layer::Help => state.help = on,
        Layer::Complete => {
            if on {
                state.complete = Some(muninn_ui::LinkComplete {
                    open: Some(0),
                    at: 0,
                    query: String::new(),
                    items: Vec::new(),
                    index: 0,
                });
            } else {
                state.complete = None;
            }
        }
        Layer::Find => {
            if on {
                state.find = Some(muninn_ui::FindState::default());
            } else {
                state.find = None;
            }
        }
        Layer::BrokenPanel => state.broken_panel = on,
        Layer::RenameConfirm => {
            if on {
                state.rename_confirm = Some(RenameConfirm {
                    id: 0,
                    name: "Alpha".to_owned(),
                    referrers: 0,
                    rewrite: true,
                });
            } else {
                state.rename_confirm = None;
            }
        }
        Layer::MoveConfirm => {
            if on {
                state.move_confirm = Some(MoveConfirm {
                    id: 0,
                    folder: "folder".to_owned(),
                    referrers: 0,
                    rewrite: true,
                });
            } else {
                state.move_confirm = None;
            }
        }
        Layer::FolderRenameConfirm => {
            if on {
                state.folder_rename_confirm = Some(FolderRenameConfirm {
                    folder: "folder".to_owned(),
                    name: "renamed".to_owned(),
                    referrers: 0,
                    rewrite: true,
                });
            } else {
                state.folder_rename_confirm = None;
            }
        }
        Layer::Menu => {
            if on {
                state.menu = Some(ContextMenu {
                    target: MenuTarget::Note(0),
                    x: 0.0,
                    y: 0.0,
                });
            } else {
                state.menu = None;
            }
        }
        Layer::NewNote => {
            if on {
                state.new_note_prompt = Some(NamePrompt {
                    folder: String::new(),
                });
            } else {
                state.new_note_prompt = None;
            }
        }
        Layer::RenameNote => {
            if on {
                state.rename_prompt = Some(RenamePrompt { id: 0 });
            } else {
                state.rename_prompt = None;
            }
        }
        Layer::NewFolder => {
            if on {
                state.new_folder_prompt = Some(FolderPrompt {
                    parent: String::new(),
                });
            } else {
                state.new_folder_prompt = None;
            }
        }
        Layer::RenameFolder => {
            if on {
                state.rename_folder_prompt = Some(FolderRename {
                    folder: "folder".to_owned(),
                });
            } else {
                state.rename_folder_prompt = None;
            }
        }
        Layer::DeleteConfirm => {
            if on {
                state.delete_confirm = Some(muninn_ui::DeleteConfirm::Note(0));
            } else {
                state.delete_confirm = None;
            }
        }
        Layer::MovePicker => {
            state.move_picker = if on { Some(0) } else { None };
        }
    }
}

const ESCAPE_ORDER: [Layer; 16] = [
    Layer::Palette,
    Layer::Settings,
    Layer::Help,
    Layer::Complete,
    Layer::Find,
    Layer::BrokenPanel,
    Layer::RenameConfirm,
    Layer::MoveConfirm,
    Layer::FolderRenameConfirm,
    Layer::Menu,
    Layer::NewNote,
    Layer::RenameNote,
    Layer::NewFolder,
    Layer::RenameFolder,
    Layer::DeleteConfirm,
    Layer::MovePicker,
];

#[test]
fn escape_peels_layers_in_documented_order() {
    let (_vault, mut state) = state();
    for layer in ESCAPE_ORDER {
        set_layer(&mut state, layer, true);
    }
    for layer in ESCAPE_ORDER {
        assert_eq!(
            state.topmost_layer(),
            Some(layer),
            "topmost layer should be {layer:?}"
        );
        state.dismiss_layer(layer);
    }
    assert_eq!(state.topmost_layer(), None, "all layers dismissed");
}

#[test]
fn dismissing_a_layer_does_not_close_others() {
    let (_vault, mut state) = state();
    set_layer(&mut state, Layer::Find, true);
    set_layer(&mut state, Layer::Help, true);
    // Help sits below Find in the stack order? No: Help is above Complete,
    // which is above Find — so Help is on top of Find.
    assert_eq!(state.topmost_layer(), Some(Layer::Help));
    state.dismiss_layer(Layer::Help);
    assert_eq!(state.topmost_layer(), Some(Layer::Find));
    state.dismiss_layer(Layer::Find);
    assert_eq!(state.topmost_layer(), None);
}
