use ::curses;

const TABWIDTH: usize = 4;

#[derive(Clone, Debug)]
pub enum Output {
	Str(String),
	AttrOn(ncurses::attr_t),
	AttrOff(ncurses::attr_t),
	Fg(usize),
	Bg(usize),
}

pub struct Preformatted {
	width: usize,
	content: Vec<Vec<Output>>,
	raw: Vec<String>,
	//mapping: 
}

impl Preformatted {
	pub fn new(width: usize) -> Self {
		Preformatted { width: width, content: vec![], raw: vec!["".to_string()] }
	}
	pub fn len(&self) -> usize {
		self.content.len()
	}
	pub fn write(&self, line: usize, p: &curses::Palette, prefix: Vec<Output>) {
		let mut all = prefix;
		all.extend(self.content[line].clone().into_iter()); // Is clone.into_iter necessary?
		all.append(&mut vec![Output::Fg(0), Output::Bg(0)]);
		let (mut curfg, mut curbg) = (0, 0);
		all.into_iter().for_each(|elem| {
			match elem {
				Output::Str(s) => { ncurses::addstr(&s); },
				Output::AttrOn(a) => { ncurses::attr_on(a); },
				Output::AttrOff(a) => { ncurses::attr_off(a); },
				Output::Fg(c) => { curfg = c; p.set(curfg, curbg); },
				Output::Bg(c) => { curbg = c; p.set(curfg, curbg); },
			}
		});
	}
	pub fn raw(&self) -> String {
		self.raw.join("/")
	}
	fn translate(&self, chunk: usize, idx: usize) -> (usize, usize) {
		unimplemented!("translate");
	}
	pub fn search(&self, q: &str) -> Vec<((usize, usize), (usize, usize))> {
		unimplemented!("search");
	}
}

#[derive(Debug)]
pub enum FmtCmd {
	Literal(String),
	Container(Vec<FmtCmd>),
	Color(usize, Box<FmtCmd>),
	NoBreak(Box<FmtCmd>),
	Exclude(Box<FmtCmd>),
}

impl FmtCmd {
	pub fn lit(s: &str) -> Self { FmtCmd::Literal(s.to_string()) }
	pub fn cat(children: Vec<Self>) -> Self { FmtCmd::Container(children) }
	pub fn color(c: usize, child: Self) -> Self { FmtCmd::Color(c, Box::new(child)) }
	pub fn nobreak(child: Self) -> Self { FmtCmd::NoBreak(Box::new(child)) }
	pub fn exclude(child: Self) -> Self { FmtCmd::Exclude(Box::new(child)) }

	fn internal_format(output: &mut Preformatted, content: &FmtCmd, startcol: usize, color: usize, color_offset: usize, record: bool) -> usize {
		let addchar = |target: &mut Vec<Output>, c: char| {
			// Ergh I can't make these all part of the same if statement because apparently the
			// borrow in the first condition remains until the end of the whole if/else sequence,
			// when I think it should be released as soon as its condition evaluates to false.
			if let Some(Output::Str(ref mut s)) = target.last_mut() { s.push(c); return; }
			if target.last().is_some() { target.push(Output::Str(c.to_string())) }
			else { target.push(Output::Str(c.to_string())) }
		};
		let append = |target: &mut Vec<Vec<Output>>, mut content: Vec<Vec<Output>>| {
			if content.len() == 0 {}
			else if target.len() == 0 { target.append(&mut content); }
			else {
				target.last_mut().unwrap().append(&mut content[0]);
				target.extend(content.into_iter().skip(1));
			}
		};
		let strappend = |target: &mut Vec<String>, mut content: Vec<String>| {
			if content.len() == 0 {}
			else if target.len() == 0 { target.append(&mut content); }
			else {
				target.last_mut().unwrap().push_str(&mut content[0]);
				target.extend(content.into_iter().skip(1));
			}
		};
		match content {
			FmtCmd::Literal(value) => {
				let mut cur = vec![Output::Fg(color)];
				let mut cnt = startcol;
				let mut need_mapping = true;
				/* Things I dislike about Rust:
				 * Jeez, I found a few lines of code that were duplicated in a couple places, so I
				 * just wanted to move them into a closure to reduce the repetition.  Is that
				 * really too much to ask?  In most sane languages, this would be a painless
				 * operation.  In Rust, I need to either wrap every variable I access in the
				 * closure in a `RefCell` (and then change everywhere I access it), or else pass
				 * every variable modified by the closure into it as a reference (impractical if
				 * I'm modifying many variables).  This greatly decreases the utility of closures
				 * as a tool for cutting down on code duplication.
				 */
				let newline = |output: &mut Preformatted, cur: &mut Vec<Output>, cnt: &mut usize, need_mapping: &mut bool| {
					cur.append(&mut vec![Output::Fg(0), Output::Bg(0)]);
					append(&mut output.content, vec![cur.clone(), vec![]]);
					*cur = vec![Output::Fg(color)];
					*cnt = 0;
					*need_mapping = true;
				};
				for c in value.chars() {
					match c {
						'\n' => newline(output, &mut cur, &mut cnt, &mut need_mapping),
						'\t' => {
							if output.width > 0 && cnt >= output.width - TABWIDTH { newline(output, &mut cur, &mut cnt, &mut need_mapping); }
							cur.push(Output::Str(std::iter::repeat(" ").take(TABWIDTH).collect::<String>())); // TODO What if width < 4?
							cnt += TABWIDTH;
							need_mapping = true;
						},
						c => {
							let cw = wcwidth::char_width(c).unwrap_or(0) as usize;
							if output.width > 0 && cnt + cw > output.width { newline(output, &mut cur, &mut cnt, &mut need_mapping); }
							addchar(&mut cur, c);
							cnt += cw;
						},
					}
					if record {
						output.raw.last_mut().unwrap().push(c); // Unwrap OK -- `raw` guaranteed non-empty
						if need_mapping && cnt > 0 {
							let add_mapping = |charlen: usize, offset: usize| {
								// TODO output.mapping[(output.raw.len() - 1, output.raw.last().unwrap().chars().count() - offset)] =
								//	if output.content.len() > 0 { (output.content.len() - 1, output.content.last().unwrap().len() + cur.chars().count() - charlen) }
								//	else { (0, cur.chars().count() - charlen) };
							};
							if c as i32 == 9 {
								add_mapping(TABWIDTH, 1);
								add_mapping(0, 0);
							}
							else {
								add_mapping(1, 1);
							}
							need_mapping = false;
						}
					}
				}
				append(&mut output.content, vec![cur]);
				cnt
			},
			FmtCmd::Container(children) => {
				let mut curcol = startcol;
				for child in children {
					curcol = Self::internal_format(output, child, curcol, color, color_offset, record);
				}
				curcol
			},
			FmtCmd::Color(newcolor, child) => {
				Self::internal_format(output, child, startcol, *newcolor + color_offset, color_offset, record)
			},
			FmtCmd::NoBreak(child) => {
				let mut sub = Preformatted::new(0);
				let sublen = Self::internal_format(&mut sub, child, 0, color, color_offset, record);
				match sub.content.len() {
					0 => startcol,
					1 => {
						let rawstart = (output.raw.len() - 1, output.raw.last().unwrap().len());
						let valstart = match output.content.last() {
							None => (0, 0),
							Some(outlast) => (output.content.len() - 1, outlast.len()),
						};
						/*for (k, v) in sub.mapping {
							output.mapping[...] = ...;
						}*/
						strappend(&mut output.raw, sub.raw);
						if output.width == 0 || sublen <= output.width - startcol {
							append(&mut output.content, sub.content);
							startcol + sublen
						}
						else {
							assert!(sublen < output.width); // TODO Support this
							output.content.append(&mut sub.content);
							sublen
						}
					},
					_ => panic!("Breaks not allowed in nobreak environment"), // TODO Support hard wraps in nobreak
				}
			},
			FmtCmd::Exclude(child) => {
				output.raw.push("".to_string());
				Self::internal_format(output, child, startcol, color, color_offset, false)
			},
		}
	}

	pub fn format(&self, width: usize, color_offset: usize) -> Preformatted {
		let mut ret = Preformatted::new(width);
		Self::internal_format(&mut ret, self, 0, 0 /* FIXME */, color_offset, true);
		ret
	}

	pub fn contains(&self, q: &str) -> bool { // Search a value without having to preformat it
		match self {
			FmtCmd::Literal(value) => value.contains(q),
			FmtCmd::Container(children) => children.iter().all(|x| x.contains(q)),
			FmtCmd::Color(_, child) => child.contains(q),
			FmtCmd::NoBreak(child) => child.contains(q),
			FmtCmd::Exclude(_) => false,
		}
	}
}
