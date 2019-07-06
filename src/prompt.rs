use ::curses;
use ::curses::Output;

struct Prompt<'a, T> {
	t: &'a mut T, // Stored reference for callback
	location: (usize, usize), // Where to start drawing the prompt
	width: usize, // Width of the text area without the prompt
	prompt: String, // Static text preceding the editing area
	history: Vec<String>, // Vector of past entries the user can scroll through
	callback: Box<FnMut(&mut T, &str)>, // Called every time the content changes
	histidx: usize, // Current location in history
	buf: Vec<char>, // Contents of editing area
	pos: usize, // Cursor position in buffer
	offset: usize, // Index in buffer of first visible character
	dispw: usize, // Graphical width of displayed portion of buffer
	dispn: usize, // Number of characters displayed
	promptw: usize, // Graphical width of prompt
	palette: &'a curses::Palette, // The color palette for drawing
}

fn charwidth(c: char) -> usize {
	match c.is_ascii_control() {
		true => 2,
		false => wcwidth::char_width(c).unwrap_or(0) as usize,
	}
}

fn graphwidth(s: &[char]) -> usize {
	s.iter().map(|c| charwidth(*c)).sum()
}
fn printchar(c: char) -> Vec<Output> {
	// TODO COLORS!
	if c.is_ascii_control() {
		let content =
			if c as i32 == 127 { "^?".to_string() }
			else { "^".to_string() + &(((c as u8) + 64) as char).to_string() };
		vec![Output::Fg(1), Output::Str(content), Output::Fg(0)]
	}
	else { vec![Output::Str(c.to_string())] }
}
fn repeat(c: char, n: usize) -> String {
	std::iter::repeat(c).take(n).collect::<String>()
}
fn move_in_line(by: isize) { // Apparently ncurses doesn't provide relative movement, so we have to simulate it
	let (y, x) = curses::curpos();
	ncurses::mv(y as i32, (x as isize + by) as i32);
}

impl<'a, T> Prompt<'a, T> {
	fn new(t: &'a mut T, location: (usize, usize), width: usize, prompt: &str, init: &str, history: Vec<String>, callback: Box<FnMut(&mut T, &str)>, palette: &'a curses::Palette) -> Self {
		let mut fullhist = history;
		fullhist.push(init.to_string());
		let histlen = fullhist.len();
		let promptw = prompt.chars().count();
		assert!(promptw < width); // TODO Handle this more gracefully
		Prompt {
			t: t,
			location: location,
			width: width - promptw,
			prompt: prompt.to_string(),
			history: fullhist,
			callback: callback,
			histidx: histlen - 1,
			buf: vec![],
			pos: 0,
			offset: 0,
			dispw: 0,
			dispn: 0,
			promptw: promptw,
			palette: palette,
		}
	}
	fn goto(&self, offset: usize) {
		ncurses::mv(self.location.0 as i32, (self.location.1 + self.promptw + offset) as i32);
	}
	fn do_callback(&mut self) {
		curses::prompt_off();
		(*self.callback)(self.t, &self.buf.iter().collect::<String>());
		curses::prompt_on();
		self.goto(graphwidth(&self.buf[self.offset..self.pos]));
	}
	fn draw_from(&mut self, offset: usize) {
		let start = std::cmp::max(offset, self.offset);
		let mut ret = vec![];
		//assert!(start < self.buf.len());
		let mut w = graphwidth(&self.buf[self.offset..start]);
		for c in self.buf[start..].iter() {
			let curw = charwidth(*c);
			if w + curw > self.width { break; }
			w += curw;
			ret.append(&mut printchar(*c));
		}
		ret.append(&mut vec![Output::Str(repeat(' ', self.width - w))]);
		Output::write(&ret, &self.palette);
	}
	fn seek(&mut self, by: isize) {
		let ndelta = std::cmp::min(std::cmp::max(self.pos as isize + by, 0), self.buf.len() as isize) - self.pos as isize;
		let delta = ndelta.abs() as usize;
		if ndelta > 0 {
			if self.dispn < self.buf.len() && self.pos + delta >= self.offset + self.dispn { // We're going off the right end
				let dispend =
					if self.pos + delta < self.buf.len() { charwidth(self.buf[self.pos + delta]) - 1 }
					else { 0 };
				self.dispw = dispend;
				self.dispn = 0;
				for c in self.buf[0..self.pos + delta].iter().rev() {
					let curw = charwidth(*c);
					if self.dispw + curw >= self.width { break; }
					self.dispw += curw;
					self.dispn += 1;
				}
				self.offset = self.pos + delta - self.dispn;
				self.goto(0);
				let offset = self.offset; // Stupid borrow requirements
				self.draw_from(offset);
				self.goto(self.dispw - dispend);
			}
			else {
				move_in_line(graphwidth(&self.buf[self.pos..self.pos + delta]) as isize);
			}
		}
		else if ndelta < 0 {
			if self.dispn < self.buf.len() && self.pos - delta < self.offset { // Going off the left end
				self.offset -= delta - (self.pos - self.offset);
				self.dispw = 0;
				self.dispn = 0;
				for c in self.buf[self.offset..].iter() {
					let curw = charwidth(*c);
					if self.dispw + curw > self.width { break; }
					self.dispw += curw;
					self.dispn += 1;
				}
				self.goto(0);
				let offset = self.offset;
				self.draw_from(offset);
				self.goto(0);
			}
			else {
				move_in_line(-(graphwidth(&self.buf[self.pos - delta..self.pos]) as isize));
			}
		}
		if ndelta > 0 { self.pos += delta; }
		else { self.pos -= delta; }
	}
	fn reset(&mut self, value: &str) {
		self.buf = value.chars().collect::<Vec<char>>();
		self.pos = 0;
		self.offset = 0;
		self.dispw = 0;
		self.dispn = 0;
		for c in self.buf.iter() {
			let curw = charwidth(*c);
			if self.dispw + curw > self.width { break }
			self.dispw += curw;
			self.dispn += 1;
		}
		ncurses::mv(self.location.0 as i32, self.location.1 as i32);
		ncurses::addstr(&(self.prompt.clone() + &repeat(' ', self.width)));
		self.goto(0);
		//curses::prompt_on();
		self.draw_from(0);
		let buflen = self.buf.len() as isize;
		self.seek(buflen);
		self.do_callback();
	}
	fn histseek(&mut self, by: isize) {
		let oldidx = self.histidx;
		let newidx = std::cmp::max(std::cmp::min(oldidx as isize + by, self.history.len() as isize - 1), 0) as usize;
		if oldidx != newidx {
			self.history[self.histidx] = self.buf.iter().collect::<String>();
			self.histidx = newidx;
			let histitem = self.history[self.histidx].clone();
			self.reset(&histitem);
		}
	}
	fn read(&mut self) -> String {
		let init = self.history.last().unwrap().clone();
		self.reset(&init);
		loop {
			match ncurses::getch() {
				0x0a => return self.buf.iter().collect::<String>(), // Enter
				0x7f => { // Backspace
					if self.pos <= 0 { continue; }
					self.seek(-1);
					let rmwidth = charwidth(self.buf[self.pos]);
					self.buf.remove(self.pos);
					self.dispw -= rmwidth;
					self.dispn -= 1;
					for c in self.buf[self.offset + self.dispn..].iter() {
						let curw = charwidth(*c);
						if self.dispw + curw > self.width { break; }
						self.dispw += curw;
						self.dispn += 1;
					}
					let pos = self.pos;
					self.draw_from(pos);
					self.do_callback();
				},
				ncurses::KEY_DC => { // Delete key
					if self.pos >= self.buf.len() { continue; }
					let rmwidth = charwidth(self.buf[self.pos]);
					self.buf.remove(self.pos);
					self.dispw -= rmwidth;
					self.dispn -= 1;
					let pos = self.pos;
					self.draw_from(pos);
					self.do_callback();
				}
				0x01 | ncurses::KEY_HOME => { let newpos = -(self.pos as isize); self.seek(newpos); }, // ^A
				0x05 | ncurses::KEY_END => { let newpos = (self.buf.len() - self.pos) as isize; self.seek(newpos); }, // ^E
				0x1b => { return "".to_string(); }, // Escape
				ncurses::KEY_RIGHT => self.seek(1),
				ncurses::KEY_LEFT => self.seek(-1),
				ncurses::KEY_UP => self.histseek(-1),
				ncurses::KEY_DOWN => self.histseek(1),
				ncurses::KEY_RESIZE => (), // TODO This needs to be handled
				key if key >= 256 => (), // Other ncurses special keys
				key => {
					let mut utf_input = vec![key as u8];
					let k = key as u8;
					if k & 0x80 != 0 { // UTF-8 input // TODO Consider moving this to curses.rs
						let charlen =
							if k & 0xf8 == 0xf0 { 3 }
							else if k & 0xf0 == 0xe0 { 2 }
							else if k & 0xe0 == 0xc0 { 1 }
							else { 0 };
						for _ in 0..charlen { utf_input.push(ncurses::getch() as u8); }
					}
					let utfstr = String::from_utf8(utf_input).unwrap();
					assert!(utfstr.chars().count() == 1);
					let c = utfstr.chars().nth(0).unwrap();
					self.buf.insert(self.pos, c);
					self.dispw += charwidth(c);
					self.dispn += 1;
					while self.dispw + charwidth(c) > self.width {
						assert!(self.buf.len() >= self.offset + self.dispn);
						self.dispw -= charwidth(self.buf[self.offset + self.dispn - 1]);
						self.dispn -= 1;
					}
					if self.pos - self.offset < self.dispn {
						let pos = self.pos;
						self.draw_from(pos);
					}
					self.seek(1);
					self.do_callback();
				},
			};
		}
	}
}

pub fn prompt<T>(t: &mut T, location: (usize, usize), width: usize, prompt: &str, init: &str, history: Vec<String>, callback: Box<FnMut(&mut T, &str)>, palette: &curses::Palette) -> String {
	curses::prompt_on();
	let ret = Prompt::<T>::new(t, location, width, prompt, init, history, callback, palette).read();
	curses::prompt_off();
	ret
}
