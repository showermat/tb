use ::curses;
use ::curses::Key;
use ::curses::Output;
use ::errors::*;

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

impl<'a, T> Prompt<'a, T> {
	fn new(t: &'a mut T, location: (usize, usize), width: usize, prompt: &str, init: &str, mut history: Vec<String>, callback: Box<FnMut(&mut T, &str)>, palette: &'a curses::Palette) -> Result<Self> {
		history.push(init.to_string());
		let histlen = history.len();
		let promptw = prompt.chars().count();
		if promptw >= width - 1 {
			bail!("Prompt string is to wide for given area");
		}
		Ok(Prompt {
			t: t,
			location: location,
			width: width - promptw,
			prompt: prompt.to_string(),
			history: history,
			callback: callback,
			histidx: histlen - 1,
			buf: vec![],
			pos: 0,
			offset: 0,
			dispw: 0,
			dispn: 0,
			promptw: promptw,
			palette: palette,
		})
	}
	fn goto(&self, offset: usize) {
		ncurses::mv(self.location.0 as i32, (self.location.1 + self.promptw + offset) as i32);
	}
	fn do_callback(&mut self) -> Result<()> {
		curses::prompt_off()?;
		(*self.callback)(self.t, &self.buf.iter().collect::<String>());
		curses::prompt_on()?;
		self.goto(graphwidth(&self.buf[self.offset..self.pos]));
		Ok(())
	}
	fn draw_from(&mut self, offset: usize) -> Result<()> {
		let start = std::cmp::max(offset, self.offset);
		let mut ret = vec![];
		let mut w = graphwidth(&self.buf[self.offset..start]);
		for c in self.buf[start..].iter() {
			let curw = charwidth(*c);
			if w + curw > self.width { break; }
			w += curw;
			ret.append(&mut printchar(*c));
		}
		ret.append(&mut vec![Output::Str(repeat(' ', self.width - w))]);
		Output::write(&ret, &self.palette)?;
		Ok(())
	}
	fn seek(&mut self, by: isize) -> Result<()> {
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
				self.draw_from(offset)?;
				self.goto(self.dispw - dispend);
			}
			else {
				curses::move_in_line(graphwidth(&self.buf[self.pos..self.pos + delta]) as isize);
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
				self.draw_from(offset)?;
				self.goto(0);
			}
			else {
				curses::move_in_line(-(graphwidth(&self.buf[self.pos - delta..self.pos]) as isize));
			}
		}
		if ndelta > 0 { self.pos += delta; }
		else { self.pos -= delta; }
		Ok(())
	}
	fn reset(&mut self, value: &str) -> Result<()> {
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
		self.draw_from(0)?;
		let buflen = self.buf.len() as isize;
		self.seek(buflen)?;
		self.do_callback()?;
		Ok(())
	}
	fn histseek(&mut self, by: isize) -> Result<()> {
		let oldidx = self.histidx;
		let newidx = std::cmp::max(std::cmp::min(oldidx as isize + by, self.history.len() as isize - 1), 0) as usize;
		if oldidx != newidx {
			self.history[self.histidx] = self.buf.iter().collect::<String>();
			self.histidx = newidx;
			let histitem = self.history[self.histidx].clone();
			self.reset(&histitem)?;
		}
		Ok(())
	}
	fn read(&mut self) -> Result<String> {
		let init = self.history.last().ok_or("Prompt history is empty")?.clone();
		self.reset(&init)?;
		loop {
			match curses::read(-1) {
				Key::Char('\x0a') => return Ok(self.buf.iter().collect::<String>()), // Enter
				Key::Char('\x7f') => { // Backspace
					if self.pos <= 0 { continue; }
					self.seek(-1)?;
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
					self.draw_from(pos)?;
					self.do_callback()?;
				},
				Key::Special(ncurses::KEY_DC) => { // Delete key
					if self.pos >= self.buf.len() { continue; }
					let rmwidth = charwidth(self.buf[self.pos]);
					self.buf.remove(self.pos);
					self.dispw -= rmwidth;
					self.dispn -= 1;
					let pos = self.pos;
					self.draw_from(pos)?;
					self.do_callback()?;
				}
				Key::Char('\x01') | Key::Special(ncurses::KEY_HOME) => { let newpos = -(self.pos as isize); self.seek(newpos)?; }, // ^A
				Key::Char('\x05') | Key::Special(ncurses::KEY_END) => { let newpos = (self.buf.len() - self.pos) as isize; self.seek(newpos)?; }, // ^E
				Key::Char('\x1b') => { return Ok("".to_string()); }, // Escape
				Key::Special(ncurses::KEY_RIGHT) => self.seek(1)?,
				Key::Special(ncurses::KEY_LEFT) => self.seek(-1)?,
				Key::Special(ncurses::KEY_UP) => self.histseek(-1)?,
				Key::Special(ncurses::KEY_DOWN) => self.histseek(1)?,
				Key::Special(ncurses::KEY_RESIZE) => (),
				Key::Char(c) => {
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
						self.draw_from(pos)?;
					}
					self.seek(1)?;
					self.do_callback()?;
				},
				_ => (),
			};
		}
	}
}

pub fn prompt<T>(t: &mut T, location: (usize, usize), width: usize, prompt: &str, init: &str, history: Vec<String>, callback: Box<FnMut(&mut T, &str)>, palette: &curses::Palette) -> Result<String> {
	curses::prompt_on()?;
	let ret = Prompt::<T>::new(t, location, width, prompt, init, history, callback, palette)?.read()?;
	curses::prompt_off()?;
	Ok(ret)
}
