use std::mem;

use imgui::sys::*;
use imgui::Io;
use imgui::{BackendFlags, Context, Key};
use std::time::Instant;
use thiserror::Error;
use winapi::shared::{
    minwindef::*,
    windef::{HICON, HWND, POINT, RECT},
};
use winapi::um::{errhandlingapi::GetLastError, winuser::*};

pub type WindowProc = unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT;

#[derive(Debug, Error)]
pub enum Win32ImplError {
    #[error("Failed to prepare frame - {0}")]
    ExternalError(String),
    #[error("Could not get IO, reference was null")]
    IoNull,
}

pub struct Win32Impl {
    hwnd: HWND,
    previous_frame_time: Instant,
    last_mouse_cursor: ImGuiMouseCursor,
}

impl Win32Impl {
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn init(imgui: &mut Context, hwnd: HWND) -> Result<Win32Impl, Win32ImplError> {
        let previous_frame_time = Instant::now();
        let io = imgui.io_mut();

        io.backend_flags |= BackendFlags::HAS_MOUSE_CURSORS; // We can honor GetMouseCursor() values (optional)
        io.backend_flags |= BackendFlags::HAS_SET_MOUSE_POS; // We can honor io.WantSetMousePos requests (optional, rarely used)

        io.key_map[Key::Tab as usize] = VK_TAB as u32;
        io.key_map[Key::LeftArrow as usize] = VK_LEFT as u32;
        io.key_map[Key::RightArrow as usize] = VK_RIGHT as u32;
        io.key_map[Key::UpArrow as usize] = VK_UP as u32;
        io.key_map[Key::DownArrow as usize] = VK_DOWN as u32;
        io.key_map[Key::PageUp as usize] = VK_PRIOR as u32;
        io.key_map[Key::PageDown as usize] = VK_NEXT as u32;
        io.key_map[Key::Home as usize] = VK_HOME as u32;
        io.key_map[Key::End as usize] = VK_END as u32;
        io.key_map[Key::Insert as usize] = VK_INSERT as u32;
        io.key_map[Key::Delete as usize] = VK_DELETE as u32;
        io.key_map[Key::Backspace as usize] = VK_BACK as u32;
        io.key_map[Key::Space as usize] = VK_SPACE as u32;
        io.key_map[Key::KeyPadEnter as usize] = VK_RETURN as u32;
        io.key_map[Key::Escape as usize] = VK_ESCAPE as u32;
        io.key_map[Key::KeyPadEnter as usize] = VK_RETURN as u32;
        io.key_map[Key::A as usize] = 'A' as u32;
        io.key_map[Key::C as usize] = 'C' as u32;
        io.key_map[Key::V as usize] = 'V' as u32;
        io.key_map[Key::X as usize] = 'X' as u32;
        io.key_map[Key::Y as usize] = 'Y' as u32;
        io.key_map[Key::Z as usize] = 'Z' as u32;

        imgui.set_platform_name(format!("imgui-win32 {}", env!("CARGO_PKG_VERSION")));
        let last_cursor = ImGuiMouseCursor_COUNT;

        Ok(Win32Impl {
            hwnd,
            previous_frame_time,
            last_mouse_cursor: last_cursor,
        })
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn prepare_frame(&mut self, context: &mut Context) -> Result<(), Win32ImplError> {
        let io = context.io_mut();

        // Setup display size (every frame to accommodate for window resizing)
        let mut rect: RECT = mem::zeroed();

        let status = GetClientRect(self.hwnd, &mut rect);
        if status == FALSE {
            return Err(Win32ImplError::ExternalError(format!(
                "GetClientRect failed with last error `{:#X}`",
                GetLastError()
            )));
        };

        let width = (rect.right - rect.left) as f32;
        let height = (rect.bottom - rect.top) as f32;
        io.display_size = [width, height];

        // Setup time step
        let current_time = Instant::now();
        let last_time = self.previous_frame_time;

        io.delta_time = current_time.duration_since(last_time).as_secs_f32();
        self.previous_frame_time = current_time;

        // Update OS mouse position
        self.update_mouse_data(io);

        // Process workarounds for known Windows key handling issues
        self.process_key_event_workarounds(io);

        // Update OS mouse cursor with the cursor requested by imgui
        let mouse_cursor = match io.mouse_draw_cursor {
            true => ImGuiMouseCursor_None,
            false => igGetMouseCursor(),
        };
        if self.last_mouse_cursor != mouse_cursor {
            self.last_mouse_cursor = mouse_cursor;
            Self::update_mouse_cursor();
        }

        // Read key states
        io.key_ctrl = (GetKeyState(VK_CONTROL) as u16 & 0x8000) != 0;
        io.key_shift = (GetKeyState(VK_SHIFT) as u16 & 0x8000) != 0;
        io.key_alt = (GetKeyState(VK_MENU) as u16 & 0x8000) != 0;
        io.key_super = false;

        Ok(())
    }

    unsafe fn update_mouse_cursor() -> bool {
        let io = match igGetIO().as_mut() {
            Some(io) => io,
            None => return false,
        };

        if io.ConfigFlags & ImGuiConfigFlags_NoMouseCursorChange as i32 != 0 {
            return false;
        }

        let imgui_cursor = igGetMouseCursor();
        if imgui_cursor == ImGuiMouseCursor_None || io.MouseDrawCursor {
            SetCursor(std::ptr::null_mut());
        } else {
            #[allow(non_upper_case_globals)]
            let cursor = match imgui_cursor {
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
            };
            SetCursor(cursor as HICON);
        }

        true
    }

    unsafe fn update_mouse_data(&self, io: &mut Io) {
        let foreground_hwnd = GetForegroundWindow();
        if foreground_hwnd != self.hwnd {
            return;
        }

        if io.want_set_mouse_pos {
            let mut pos = POINT {
                x: io.mouse_pos[0] as i32,
                y: io.mouse_pos[1] as i32,
            };

            if ClientToScreen(self.hwnd, &mut pos) == TRUE {
                SetCursorPos(pos.x, pos.y);
            }
        }
    }

    unsafe fn process_key_event_workarounds(&self, io: &mut Io) {}
}

#[allow(clippy::missing_safety_doc)]
/// Call this function in WndProc
pub unsafe fn imgui_win32_window_proc(
    window: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Result<(), Win32ImplError> {
    let io = match igGetIO().as_mut() {
        Some(io) => io,
        None => return Err(Win32ImplError::IoNull),
    };

    // awful after fmt but it works i guess
    match msg {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN
        | WM_MBUTTONDBLCLK => {
            let mut button = 0;
            if msg == WM_LBUTTONDOWN || msg == WM_LBUTTONDBLCLK {
                button = 0;
            }
            if msg == WM_RBUTTONDOWN || msg == WM_RBUTTONDBLCLK {
                button = 1;
            }
            if msg == WM_MBUTTONDOWN || msg == WM_MBUTTONDBLCLK {
                button = 2;
            }
            if msg == WM_XBUTTONDOWN || msg == WM_XBUTTONDBLCLK {
                button = if GET_XBUTTON_WPARAM(wparam) == XBUTTON1 {
                    3
                } else {
                    4
                }
            }

            if !igIsAnyMouseDown() && GetCapture().is_null() {
                SetCapture(window);
            }

            io.MouseDown[button] = true;
        }

        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
            let mut button = 0;
            if msg == WM_LBUTTONUP {
                button = 0;
            }
            if msg == WM_RBUTTONUP {
                button = 1;
            }
            if msg == WM_MBUTTONUP {
                button = 2;
            }
            if msg == WM_XBUTTONUP {
                button = if GET_XBUTTON_WPARAM(wparam) == XBUTTON1 {
                    3
                } else {
                    4
                }
            }

            io.MouseDown[button] = false;
            if !igIsAnyMouseDown() && GetCapture() == window {
                ReleaseCapture();
            }
        }

        WM_MOUSEWHEEL => {
            io.MouseWheel += (GET_WHEEL_DELTA_WPARAM(wparam) / WHEEL_DELTA) as f32;
        }

        WM_MOUSEHWHEEL => {
            io.MouseWheelH += (GET_WHEEL_DELTA_WPARAM(wparam) / WHEEL_DELTA) as f32;
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if wparam < 256 {
                io.KeysDown[wparam] = true;
            }
        }

        WM_KEYUP | WM_SYSKEYUP => {
            if wparam < 256 {
                io.KeysDown[wparam] = false;
            }
        }

        WM_CHAR => {
            if wparam > 0 && wparam < 0x10000 {
                let ig_io = igGetIO();
                ImGuiIO_AddInputCharacterUTF16(ig_io, wparam as u16);
            }
        }

        WM_SETCURSOR => {
            if LOWORD(lparam as u32) as isize == HTCLIENT {
                Win32Impl::update_mouse_cursor();
            }
        }

        // currently no gamepad support
        WM_DEVICECHANGE => {}

        _ => return Ok(()),
    };

    Ok(())
}
