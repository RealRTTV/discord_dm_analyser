#![allow(dead_code)]

use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::ptr::null_mut;

#[repr(C)]
pub struct ConsoleScreenBufferInfo {
    pub size: Coord,
    pub cursor_pos: Coord,
    pub attributes: u16,
    pub window: SmallRectangle,
    pub maximum_window_size: Coord,
}

#[repr(C)]
pub struct Coord {
    pub x: i16,
    pub y: i16,
}

#[repr(C)]
pub struct SmallRectangle {
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
}

impl From<(i16, i16)> for Coord {
    fn from(value: (i16, i16)) -> Self {
        Self {
            x: value.0,
            y: value.1
        }
    }
}

impl Into<(i16, i16)> for Coord {
    fn into(self) -> (i16, i16) {
        (self.x, self.y)
    }
}

#[link(name = "msvcrt")]
unsafe extern "C" {
    fn _getch() -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    #[allow(improper_ctypes)]
    fn SetConsoleCursorPosition(handle: *const c_void, pos: Coord) -> bool;

    fn SetConsoleTextAttribute(handle: *const c_void, attribs: u16) -> bool;

    fn WriteConsoleA(handle: *const c_void, ptr: *const c_void, len: u32, num_chars_written: *mut u32, reserved: *mut c_void) -> bool;

    fn GetStdHandle(id: u32) -> *mut c_void;

    fn GetConsoleScreenBufferInfo(handle: *mut c_void, console_screen_buffer_info: &mut MaybeUninit<ConsoleScreenBufferInfo>) -> bool;
}

fn stdout_handle() -> *mut c_void {
    unsafe { GetStdHandle(-11_i32 as u32) }
}

pub fn set_cursor(x: usize, y: usize) {
    unsafe { SetConsoleCursorPosition(stdout_handle(), Coord::from((x as i16, y as i16))); }
}

pub fn set_color(color: u16) {
    unsafe { SetConsoleTextAttribute(stdout_handle(), color); }
}

pub fn stdout_str(str: &str) {
    unsafe { WriteConsoleA(stdout_handle(), str.as_ptr().cast::<c_void>(), str.len() as u32, null_mut(), null_mut()); }
}

pub fn get_char() -> i32 {
    unsafe { _getch() }
}

pub fn get_console_screen_buffer_info() -> ConsoleScreenBufferInfo {
    let mut console_screen_buffer_info = MaybeUninit::uninit();
    unsafe { GetConsoleScreenBufferInfo(stdout_handle(), &mut console_screen_buffer_info); }
    unsafe { console_screen_buffer_info.assume_init() }
}
