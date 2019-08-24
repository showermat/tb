extern crate ncurses;
extern crate libc;
extern crate libc_stdhandle;

// https://invisible-island.net/ncurses/man/ncurses.3x.html

use self::ncurses::*;
use self::libc_stdhandle::*;
use std::ffi::CString;
use ::interface::Color;

pub fn prompt_on() {
	curs_set(CURSOR_VISIBILITY::CURSOR_VISIBLE);
	mousemask(0, None);
}

pub fn prompt_off() {
	curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);
	mousemask((BUTTON1_PRESSED | BUTTON4_PRESSED | BUTTON5_PRESSED) as u32, None);
	mouseinterval(0);
}

pub fn setup() { // TODO Check all the results of ncurses functions that can fail -- here and elsewhere in the code
	unsafe {
		let cstr = |s: &str| { CString::new(s).expect("Tried to create null C string").into_raw() };
		let path = cstr("/dev/tty");
		let mode = cstr("r+");
		let empty = cstr("");
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
	if !has_colors() { panic!("This terminal does not support color"); } // TODO Return a result -- even better, support monochrome mode
	start_color();
	idlok(stdscr(), true);
	scrollok(stdscr(), true);
	leaveok(stdscr(), false);
	prompt_off();
	set_escdelay(100);
}

pub fn cleanup() {
	endwin();
}

pub enum Key {
	Timeout,
	Invalid,
	Char(char),
	Special(i32),
}

pub fn read(timeout: i32) -> Key { // Read a UTF-8 char from input
	ncurses::timeout(timeout);
	let ret = match ncurses::getch() {
		ncurses::ERR => Key::Timeout,
		key if key < 128 => Key::Char(key as u8 as char),
		key if key >= 256 => Key::Special(key),
		key => {
			let k = key as u8;
			let mut utf_input = vec![k];
			let charlen =
				if k & 0xf8 == 0xf0 { 3 }
				else if k & 0xf0 == 0xe0 { 2 }
				else if k & 0xe0 == 0xc0 { 1 }
				else { 0 };
			for _ in 0..charlen { utf_input.push(ncurses::getch() as u8); }
			let utfstr = String::from_utf8(utf_input);
			if let Ok(utf) = utfstr {
				if utf.chars().count() == 1 {
					Key::Char(utf.chars().next().expect("Could not pull from non-empty iterator"))
				}
				else { Key::Invalid }
			}
			else { Key::Invalid }
		}
	};
	ncurses::timeout(-1);
	ret
}

pub fn ncstr(s: &str) -> Vec<i32> {
	s.chars().map(|c| c as i32).collect()
}

#[derive(Clone)]
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
	pub fn set(&self, fg: usize, bg: usize, fillchar: char) {
		let pair = self.pairnum(fg, bg);
		ncurses::color_set(pair);
		ncurses::bkgdset(fillchar as u32 | ncurses::COLOR_PAIR(pair));
	}
}

#[derive(Clone, Copy)]
pub struct Size {
	pub w: usize,
	pub h: usize,
}

pub fn scrsize() -> Size {
	Size { w: COLS() as usize, h: LINES() as usize }
}

pub fn curpos() -> (usize, usize) {
	let (mut y, mut x) = (0, 0);
	getyx(stdscr(), &mut y, &mut x);
	(y as usize, x as usize)
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

#[derive(Clone, Debug)]
pub enum Output {
	Str(String),
//	AttrOn(ncurses::attr_t),
//	AttrOff(ncurses::attr_t),
	Fg(usize),
	Bg(usize),
//	Move(usize, usize),
	Fill(char),
}

impl Output {
	pub fn write(line: &[Output], p: &Palette) {
		let (mut curfg, mut curbg) = (0, 0);
		line.iter().for_each(|elem| {
			match elem {
				Output::Str(s) => { ncurses::addstr(&s); },
//				Output::AttrOn(a) => { ncurses::attr_on(*a); },
//				Output::AttrOff(a) => { ncurses::attr_off(*a); },
				Output::Fg(c) => { curfg = *c; p.set(curfg, curbg, ' '); },
				Output::Bg(c) => { curbg = *c; p.set(curfg, curbg, ' '); },
//				Output::Move(y, x) => { ncurses::mv(*y as i32, *x as i32); },
				Output::Fill(c) => { p.set(curfg, curbg, *c); ncurses::clrtoeol(); },
			}
		});
	}
}