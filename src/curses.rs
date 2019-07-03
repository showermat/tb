extern crate ncurses;
extern crate libc;
extern crate libc_stdhandle;

use self::ncurses::*;
use self::libc_stdhandle::*;
use std::ffi::CString;

pub fn setup() { // TODO Check all the results of ncurses functions that can fail -- here and elsewhere in the code
	//initscr();
	unsafe {
		let path = CString::new("/dev/tty").unwrap().into_raw();
		let mode = CString::new("r+").unwrap().into_raw();
		let empty = CString::new("").unwrap().into_raw();
		libc::setlocale(libc::LC_ALL, empty);
		let tty = libc::fopen(path, mode);
		let _ = CString::from_raw(path);
		let _ = CString::from_raw(mode);
		let _ = CString::from_raw(empty);
		let _oldterm = set_term(newterm(None, tty, stdout()));
	}
	keypad(stdscr(), true);
	cbreak();
	noecho();
	curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);
	if !has_colors() { panic!("This terminal does not support color"); } // TODO Return a result -- even better, support monochrome mode
	start_color();
	idlok(stdscr(), true);
	scrollok(stdscr(), true);
	leaveok(stdscr(), true);
	mousemask(ALL_MOUSE_EVENTS as u32, None);
}

pub struct Color {
	pub c8: u8,
	pub c256: u8,
}

pub struct Palette {
	fg: Vec<Color>,
	bg: Vec<Color>,
}

impl Palette {
	fn pairnum(&self, fg: usize, bg: usize) -> i16 {
		(bg * self.fg.len() + fg + 1) as i16
	}
	pub fn new(fglist: Vec<Color>, bglist: Vec<Color>) -> Self {
		fn getcol(c: &Color) -> i16 {
			( if ncurses::COLORS() >= 256 { c.c256 }
			else { c.c8 } ) as i16
		}
		let ret = Self { fg: fglist, bg: bglist };
		for (i, bgcol) in ret.bg.iter().enumerate() {
			for (j, fgcol) in ret.fg.iter().enumerate() {
				ncurses::init_pair(ret.pairnum(j, i), getcol(fgcol), getcol(bgcol));
			}
		}
		ret
	}
	pub fn set(&self, fg: usize, bg: usize) {
		ncurses::color_set(self.pairnum(fg, bg));
	}
}

pub fn cleanup() {
	endwin();
}

#[derive(Clone, Copy)]
pub struct Size {
	pub w: usize,
	pub h: usize,
}

pub fn scrsize() -> Size {
	Size { w: COLS() as usize, h: LINES() as usize }
}

#[derive(Clone, Debug)]
pub enum MouseClick { Press, Release, Click, DoubleClick, TripleClick }

#[derive(Clone, Debug)]
pub struct MouseEvent {
	pub x: u32,
	pub y: u32,
	pub button: u8,
	pub kind: MouseClick,
}

impl MouseEvent {
	pub fn new(e: &MEVENT) -> Self {
		use self::MouseClick::*;
		let b = e.bstate as i32;
		let (button, kind) =
			if b & BUTTON1_PRESSED != 0 { (1, Press) }
			else if b & BUTTON1_RELEASED != 0 { (1, Release) }
			else if b & BUTTON1_CLICKED != 0 { (1, Click) }
			else if b & BUTTON1_DOUBLE_CLICKED != 0 { (1, DoubleClick) }
			else if b & BUTTON1_TRIPLE_CLICKED != 0 { (1, TripleClick) }
			else if b & BUTTON2_PRESSED != 0 { (2, Press) }
			else if b & BUTTON2_RELEASED != 0 { (2, Release) }
			else if b & BUTTON2_CLICKED != 0 { (2, Click) }
			else if b & BUTTON2_DOUBLE_CLICKED != 0 { (2, DoubleClick) }
			else if b & BUTTON2_TRIPLE_CLICKED != 0 { (2, TripleClick) }
			else if b & BUTTON3_PRESSED != 0 { (3, Press) }
			else if b & BUTTON3_RELEASED != 0 { (3, Release) }
			else if b & BUTTON3_CLICKED != 0 { (3, Click) }
			else if b & BUTTON3_DOUBLE_CLICKED != 0 { (3, DoubleClick) }
			else if b & BUTTON3_TRIPLE_CLICKED != 0 { (3, TripleClick) }
			else if b & BUTTON4_PRESSED != 0 { (4, Press) }
			else if b & BUTTON4_RELEASED != 0 { (4, Release) }
			else if b & BUTTON4_CLICKED != 0 { (4, Click) }
			else if b & BUTTON4_DOUBLE_CLICKED != 0 { (4, DoubleClick) }
			else if b & BUTTON4_TRIPLE_CLICKED != 0 { (4, TripleClick) }
			else if b & BUTTON5_PRESSED != 0 { (5, Press) }
			else if b & BUTTON5_RELEASED != 0 { (5, Release) }
			else if b & BUTTON5_CLICKED != 0 { (5, Click) }
			else if b & BUTTON5_DOUBLE_CLICKED != 0 { (5, DoubleClick) }
			else if b & BUTTON5_TRIPLE_CLICKED != 0 { (5, TripleClick) }
			else { panic!("Unknown button state in mouse event"); }
		;
		Self { x: e.x as u32, y: e.y as u32, button: button, kind: kind }
	}
}

pub fn mouseevents() -> Vec<MouseEvent> {
	let mut ret = vec![];
	let mut event = ncurses::MEVENT { id: 0, x: 0, y: 0, z: 0, bstate: 0 };
	while getmouse(&mut event) == OK {
		ret.push(MouseEvent::new(&event));
	}
	ret.reverse();
	ret
}
