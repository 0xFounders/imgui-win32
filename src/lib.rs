use imgui::{
    sys::{
        igGetIO, igGetMouseCursor, igIsAnyMouseDown, ImGuiConfigFlags_NoMouseCursorChange,
        ImGuiIO_AddInputCharacterUTF16, ImGuiMouseCursor, ImGuiMouseCursor_Arrow,
        ImGuiMouseCursor_Hand, ImGuiMouseCursor_None, ImGuiMouseCursor_NotAllowed,
        ImGuiMouseCursor_ResizeAll, ImGuiMouseCursor_ResizeEW, ImGuiMouseCursor_ResizeNESW,
        ImGuiMouseCursor_ResizeNS, ImGuiMouseCursor_ResizeNWSE, ImGuiMouseCursor_TextInput,
    },
    BackendFlags, Context, Key,
};
use std::time::Instant;
use thiserror::Error;
use windows::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ClientToScreen, ScreenToClient},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub type WindowProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

pub enum ProcResponse {
    NoAction,
    ActionTaken,
}

pub struct Win32Impl {
    hwnd: HWND,
    time: Instant,
    last_cursor: ImGuiMouseCursor,
}

#[inline]
fn loword(l: u32) -> u16 {
    (l & 0xffff) as u16
}

#[inline]
fn hiword(l: u32) -> u16 {
    ((l >> 16) & 0xffff) as u16
}

fn get_xbutton_wparam(w_param: u32) -> MOUSEHOOKSTRUCTEX_MOUSE_DATA {
    MOUSEHOOKSTRUCTEX_MOUSE_DATA(hiword(w_param) as u32)
}

fn get_wheel_delta_wparam(w_param: u32) -> u32 {
    hiword(w_param) as u32
}

impl Win32Impl {
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn init(imgui: &mut Context, hwnd: HWND) -> Result<Win32Impl, Win32ImplError> {
        let time = Instant::now();
        let io = imgui.io_mut();

        io.backend_flags.insert(BackendFlags::HAS_MOUSE_CURSORS);
        io.backend_flags.insert(BackendFlags::HAS_SET_MOUSE_POS);

        io.key_map[Key::Tab as usize] = VK_TAB.0 as u32;
        io.key_map[Key::LeftArrow as usize] = VK_LEFT.0 as u32;
        io.key_map[Key::RightArrow as usize] = VK_RIGHT.0 as u32;
        io.key_map[Key::UpArrow as usize] = VK_UP.0 as u32;
        io.key_map[Key::DownArrow as usize] = VK_DOWN.0 as u32;
        io.key_map[Key::PageUp as usize] = VK_PRIOR.0 as u32;
        io.key_map[Key::PageDown as usize] = VK_NEXT.0 as u32;
        io.key_map[Key::Home as usize] = VK_HOME.0 as u32;
        io.key_map[Key::End as usize] = VK_END.0 as u32;
        io.key_map[Key::Insert as usize] = VK_INSERT.0 as u32;
        io.key_map[Key::Delete as usize] = VK_DELETE.0 as u32;
        io.key_map[Key::Backspace as usize] = VK_BACK.0 as u32;
        io.key_map[Key::Space as usize] = VK_SPACE.0 as u32;
        io.key_map[Key::KeyPadEnter as usize] = VK_RETURN.0 as u32;
        io.key_map[Key::Escape as usize] = VK_ESCAPE.0 as u32;
        io.key_map[Key::KeyPadEnter as usize] = VK_RETURN.0 as u32;
        io.key_map[Key::A as usize] = 'A' as u32;
        io.key_map[Key::C as usize] = 'C' as u32;
        io.key_map[Key::V as usize] = 'V' as u32;
        io.key_map[Key::X as usize] = 'X' as u32;
        io.key_map[Key::Y as usize] = 'Y' as u32;
        io.key_map[Key::Z as usize] = 'Z' as u32;

        imgui.set_platform_name(format!("imgui-win32 {}", env!("CARGO_PKG_VERSION")));

        let last_cursor = ImGuiMouseCursor_None;

        Ok(Win32Impl {
            hwnd,
            time,
            last_cursor,
        })
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn prepare_frame(&mut self, context: &mut Context) -> Result<(), Win32ImplError> {
        let io = context.io_mut();

        // Set up display size every frame to handle resizing
        let mut rect: RECT = std::mem::zeroed();
        if GetClientRect(self.hwnd, &mut rect).as_bool() == false {
            return Err(Win32ImplError::ExternalError(format!(
                "GetClientRect failed with last error `{:#?}`",
                GetLastError()
            )));
        };

        let width = (rect.right - rect.left) as f32;
        let height = (rect.bottom - rect.top) as f32;
        io.display_size = [width, height];

        // Perform time step
        let current_time = Instant::now();
        let last_time = self.time;
        io.delta_time = current_time.duration_since(last_time).as_secs_f32();
        self.time = current_time;

        // Read key states
        io.key_ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
        io.key_shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
        io.key_alt = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
        io.key_super = false;

        // Mouse cursor pos and icon updates
        let current_cursor = match io.mouse_draw_cursor {
            true => ImGuiMouseCursor_None,
            false => igGetMouseCursor(),
        };

        self.update_cursor_pos(context);
        if self.last_cursor != current_cursor {
            self.last_cursor = current_cursor;
            update_cursor();
        }

        Ok(())
    }

    unsafe fn update_cursor_pos(&self, context: &mut Context) {
        let io = context.io_mut();

        if io.want_set_mouse_pos {
            let x = io.mouse_pos[0] as i32;
            let y = io.mouse_pos[1] as i32;
            let mut pos = POINT { x, y };

            if ClientToScreen(self.hwnd, &mut pos).as_bool() == true {
                SetCursorPos(pos.x, pos.y);
            }
        }

        io.mouse_pos = [-f32::MAX, -f32::MAX];
        let mut pos: POINT = std::mem::zeroed();
        let foreground_hwnd = GetForegroundWindow();
        if (self.hwnd == foreground_hwnd || IsChild(foreground_hwnd, self.hwnd).as_bool() == true)
            && GetCursorPos(&mut pos).as_bool() == true
            && ScreenToClient(self.hwnd, &mut pos).as_bool() == true
        {
            io.mouse_pos = [pos.x as f32, pos.y as f32];
        };
    }
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn imgui_win32_window_proc(
    window: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> Result<ProcResponse, Win32ImplError> {
    let io = match igGetIO().as_mut() {
        Some(io) => io,
        None => return Err(Win32ImplError::NullIO),
    };

    let w_param = w_param.0 as u32;

    let result = match msg {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN
        | WM_MBUTTONDBLCLK => {
            let button = match msg {
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => 0,
                WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => 1,
                WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => 2,
                WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => {
                    if get_xbutton_wparam(w_param) == XBUTTON1 {
                        3
                    } else {
                        4
                    }
                }
                _ => 0,
            };

            if !igIsAnyMouseDown() && GetCapture().0 == 0 {
                SetCapture(window);
            }

            io.MouseDown[button] = true;
            ProcResponse::NoAction
        }

        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
            let button = match msg {
                WM_LBUTTONUP => 0,
                WM_RBUTTONUP => 1,
                WM_MBUTTONUP => 2,
                WM_XBUTTONUP => {
                    if get_xbutton_wparam(w_param) == XBUTTON1 {
                        3
                    } else {
                        4
                    }
                }
                _ => 0,
            };

            io.MouseDown[button] = false;
            if !igIsAnyMouseDown() && GetCapture() == window {
                ReleaseCapture();
            }
            ProcResponse::NoAction
        }

        WM_MOUSEWHEEL => {
            io.MouseWheel += (get_wheel_delta_wparam(w_param) / WHEEL_DELTA) as f32;
            ProcResponse::NoAction
        }

        WM_MOUSEHWHEEL => {
            io.MouseWheelH += (get_wheel_delta_wparam(w_param) / WHEEL_DELTA) as f32;
            ProcResponse::NoAction
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if w_param < 256 {
                io.KeysDown[w_param as usize] = true;
            }
            ProcResponse::NoAction
        }

        WM_KEYUP | WM_SYSKEYUP => {
            if w_param < 256 {
                io.KeysDown[w_param as usize] = false;
            }
            ProcResponse::NoAction
        }

        WM_CHAR => {
            if w_param > 0 && w_param < 0x10000 {
                let ig_io = igGetIO();
                ImGuiIO_AddInputCharacterUTF16(ig_io, w_param as u16);
            }
            ProcResponse::NoAction
        }

        WM_SETCURSOR => {
            if loword(l_param.0 as u32) as u32 == HTCLIENT && update_cursor() {
                ProcResponse::ActionTaken
            } else {
                ProcResponse::NoAction
            }
        }

        WM_DEVICECHANGE => ProcResponse::NoAction,
        _ => ProcResponse::NoAction,
    };

    Ok(result)
}

unsafe fn update_cursor() -> bool {
    let io = match igGetIO().as_mut() {
        Some(io) => io,
        None => return false,
    };

    if io.ConfigFlags & ImGuiConfigFlags_NoMouseCursorChange as i32 != 0 {
        return false;
    };

    let mouse_cursor = igGetMouseCursor();
    let win32_cursor = if mouse_cursor == ImGuiMouseCursor_None || io.MouseDrawCursor {
        HCURSOR(0)
    } else {
        #[allow(non_upper_case_globals)]
        HCURSOR(
            match mouse_cursor {
                ImGuiMouseCursor_Arrow => IDC_ARROW,
                ImGuiMouseCursor_TextInput => IDC_IBEAM,
                ImGuiMouseCursor_ResizeAll => IDC_SIZEALL,
                ImGuiMouseCursor_ResizeEW => IDC_SIZEWE,
                ImGuiMouseCursor_ResizeNS => IDC_SIZENS,
                ImGuiMouseCursor_ResizeNESW => IDC_SIZENESW,
                ImGuiMouseCursor_ResizeNWSE => IDC_SIZENWSE,
                ImGuiMouseCursor_Hand => IDC_HAND,
                ImGuiMouseCursor_NotAllowed => IDC_NO,
                _ => IDC_ARROW,
            }
            .0 as isize,
        )
    };

    SetCursor(win32_cursor);
    true
}

#[derive(Debug, Error)]
pub enum Win32ImplError {
    #[error("Failed to prepare frame - {0}")]
    ExternalError(String),
    #[error("Could not get IO, reference was null")]
    NullIO,
}
