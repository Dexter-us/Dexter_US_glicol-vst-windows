//! Native dialogs are posted outside egui/baseview's borrowed on_frame callback.
use crate::editor::EditorState;
use crate::program_files::{self, FileAction};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use winapi::shared::windef::HWND;
use winapi::um::commdlg::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use winapi::um::winuser::{
    IsWindow, MessageBoxW, PostMessageW, SetFocus, IDCANCEL, IDYES, MB_DEFBUTTON3, MB_ICONQUESTION,
    MB_YESNOCANCEL, WM_APP,
};

pub const FILE_MESSAGE: u32 = WM_APP + 0x474;

pub fn post_request(owner: HWND, action: FileAction) -> Result<(), String> {
    let value = match action {
        FileAction::Save => 1,
        FileAction::Load => 2,
    };
    if owner.is_null() || unsafe { PostMessageW(owner, FILE_MESSAGE, value, 0) } == 0 {
        Err(format!(
            "Could not open file dialog: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn choose(owner: HWND, save: bool, current: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let mut name = vec![0u16; 32768];
    if let Some(path) = current {
        let initial: Vec<u16> = path.as_os_str().encode_wide().collect();
        if initial.len() >= name.len() {
            return Err("Filename is too long for the file dialog.".into());
        }
        name[..initial.len()].copy_from_slice(&initial);
    }
    let filter = wide("Text programs (*.txt;*.glicol)\0*.txt;*.glicol\0All files (*.*)\0*.*\0");
    let extension = wide("txt");
    let title = wide(if save {
        "Save Glicol editor text"
    } else {
        "Load Glicol editor text"
    });
    let mut options: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    options.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    options.hwndOwner = owner;
    options.lpstrFile = name.as_mut_ptr();
    options.nMaxFile = name.len() as u32;
    options.lpstrFilter = filter.as_ptr();
    options.nFilterIndex = 1;
    options.lpstrDefExt = extension.as_ptr();
    options.lpstrTitle = title.as_ptr();
    options.Flags = OFN_EXPLORER
        | OFN_NOCHANGEDIR
        | OFN_PATHMUSTEXIST
        | if save {
            OFN_OVERWRITEPROMPT
        } else {
            OFN_FILEMUSTEXIST
        };
    let accepted = unsafe {
        if save {
            GetSaveFileNameW(&mut options)
        } else {
            GetOpenFileNameW(&mut options)
        }
    };
    if accepted == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!("Windows file dialog failed (code 0x{error:x})."))
        };
    }
    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &name[..end],
    ))))
}

fn save_current(owner: HWND, state: &Arc<Mutex<EditorState>>) -> Result<bool, String> {
    let (code, current) = {
        let state = state.lock().unwrap();
        (state.code.clone(), state.file_path.clone())
    };
    let Some(path) = choose(owner, true, current.as_deref())? else {
        return Ok(false);
    };
    program_files::save(&path, &code).map_err(|e| format!("Save failed: {e}"))?;
    let mut state = state.lock().unwrap();
    state.file_snapshot = Some(code);
    state.file_path = Some(path.clone());
    state.file_status = Some(format!("Saved {}", path.display()));
    Ok(true)
}

fn perform(owner: HWND, state: &Arc<Mutex<EditorState>>, action: FileAction) -> Result<(), String> {
    if action == FileAction::Save {
        if !save_current(owner, state)? {
            state.lock().unwrap().file_status = Some("Save canceled.".into());
        }
        return Ok(());
    }
    let dirty = {
        let state = state.lock().unwrap();
        !state.code.is_empty() && state.file_snapshot.as_deref() != Some(state.code.as_str())
    };
    if dirty {
        let text = wide("Save the current editor text before loading another file?\n\nYes: save first. No: replace the editor text. Cancel: keep it unchanged.");
        let title = wide("Glicol — protect current text");
        let answer = unsafe {
            MessageBoxW(
                owner,
                text.as_ptr(),
                title.as_ptr(),
                MB_YESNOCANCEL | MB_ICONQUESTION | MB_DEFBUTTON3,
            )
        };
        if answer == 0 {
            return Err("Could not show the unsaved-text confirmation.".into());
        }
        if answer == IDCANCEL || (answer == IDYES && !save_current(owner, state)?) {
            state.lock().unwrap().file_status = Some("Load canceled; current text kept.".into());
            return Ok(());
        }
    }
    let current = state.lock().unwrap().file_path.clone();
    let Some(path) = choose(owner, false, current.as_deref())? else {
        state.lock().unwrap().file_status = Some("Load canceled; current text kept.".into());
        return Ok(());
    };
    let code =
        program_files::load(&path).map_err(|e| format!("Load failed; current text kept: {e}"))?;
    state.lock().unwrap().apply_file(path, code);
    Ok(())
}

pub fn handle_request(owner: HWND, state: Arc<Mutex<EditorState>>, action: FileAction) {
    // Never hold this mutex across a modal dialog: its nested message loop can
    // repaint the editor. The posted message also runs outside baseview's RefCell.
    state.lock().unwrap().file_busy = true;
    let result = perform(owner, &state, action);
    let mut ui = state.lock().unwrap();
    if let Err(error) = result {
        ui.file_status = Some(error);
    }
    ui.file_busy = false;
    drop(ui);
    if unsafe { IsWindow(owner) } != 0 {
        unsafe {
            SetFocus(owner);
        }
    }
}
