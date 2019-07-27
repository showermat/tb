use ::curses;
use ::curses::Output;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Bound;
use ::regex::Regex;

const TABWIDTH: usize = 4;

pub struct Search {
	query: String,
	matches: BTreeMap<usize, BTreeMap<usize, BTreeSet<(usize, usize)>>>, // line, item, start, end // TODO Consider BTreeMap<(usize, usize), BTreeSet<(usize, usize)>> or similar
}

impl Search {
	pub fn matchlines(&self) -> Vec<usize> {
		self.matches.iter().map(|(k, _)| *k).collect::<Vec<usize>>()
	}
	pub fn query(&self) -> String {
		self.query.clone()
	}
}

pub struct Preformatted {
	width: usize,
	content: Vec<Vec<Output>>,
	raw: Vec<String>,
	mapping: BTreeMap<(usize, usize), (usize, usize, usize)>,
}

impl Preformatted {
	pub fn new(width: usize) -> Self {
		Preformatted { width: width, content: vec![], raw: vec!["".to_string()], mapping: BTreeMap::new() }
	}
	pub fn len(&self) -> usize {
		self.content.len()
	}
	pub fn write(&self, line: usize, p: &curses::Palette, prefix: Vec<Output>, bg: usize, search: &Option<Search>) {
		// TODO With the way this and `highlight` are implemented, we've restricted ourselves to
		// one background color for each `Preformatted`, and this is not exposed to the `DispValue`
		// implementer.  We need to do some significant re-implementation to expose background
		// colors and attributes to the `DispValue`, and to make those work efficiently with
		// highlighting.
		let highlight = 2; // TODO Don't hardcode
		let mut all = prefix;
		let maybe_line = match search {
			Some(info) => info.matches.get(&line),
			None => None,
		};
		let content = match maybe_line { // TODO Extend to foreground color and attrs as well
			Some(matches) => {
				self.content[line].iter().enumerate().flat_map(|(i, item)| {
					match matches.get(&i) {
						None => vec![item.clone()],
						Some(regions) => {
							match item {
								Output::Str(s) => {
									let mut ret = vec![];
									let mut last = 0;
									for (start, end) in regions {
										ret.append(&mut vec![
											Output::Str(s[last..*start].to_string()),
											Output::Bg(highlight),
											Output::Str(s[*start..*end].to_string()),
											Output::Bg(bg),
										]);
										last = *end;
									}
									ret.push(Output::Str(s[last..].to_string()));
									ret
								},
								_ => panic!("Tried to highlight within a non-string"),
							}
						},
					}
				}).collect::<Vec<Output>>()
			},
			None => self.content[line].clone(),
		};
		all.push(Output::Bg(bg));
		all.extend(content);
		all.append(&mut vec![Output::Fg(0), Output::Bg(0)]);
		Output::write(&all, p);
	}
	pub fn raw(&self) -> String { // For debug
		self.raw.join("/")
	}
	fn translate(&self, chunk: usize, idx: usize) -> (usize, usize, usize) {
		let (k, v) = self.mapping.range((Bound::Unbounded, Bound::Included((chunk, idx)))).rev().next().expect("No format chunk contains the requested index");
		assert!(k.0 == chunk);
		let delta = idx - k.1;
		(v.0, v.1, v.2 + delta)
	}
	pub fn search(&self, query: &str) -> Search {
		let matchmap = match self.mapping.is_empty() {
			true => BTreeMap::new(), // No searchable content in this node, so no matches possible
			false => {
				// Get absolute start-end pairs for each match
				/*let mut matches = self.raw.iter().enumerate().flat_map(|(i, chunk)| {
					chunk.match_indices(query).map(|res| (self.translate(i, res.0), self.translate(i, res.0 + res.1.chars().count())))
				}).peekable();*/
				// The below is a standin until I get the above to work
				let regex = Regex::new(query).unwrap_or(Regex::new(&regex::escape(query)).expect("Regex construction failed even after escaping"));
				let mut matchvec = vec![];
				for (i, chunk) in self.raw.iter().enumerate() {
					for res in regex.find_iter(chunk) {
						matchvec.push((self.translate(i, res.start()), self.translate(i, res.end())));
					}
				}
				let mut matches = matchvec.iter().peekable();

				// Convert start-end pairs into start and end indices for each string in `content`
				let mut splitpairs = vec![];
				let mut on = false;
				let getlineitem = |loc: &(usize, usize, usize)| (loc.0, loc.1);
				for (i, line) in self.content.iter().enumerate() {
					for (j, item) in line.iter().enumerate() {
						if let Output::Str(s) = item {
							loop {
								if on {
									let curend = matches.peek().expect("Lost closing match in search").1;
									if getlineitem(&curend) > (i, j) {
										splitpairs.push((i, j, 0, s.chars().count()));
										break;
									}
									else {
										splitpairs.push((i, j, 0, curend.2));
										on = false;
										matches.next();
									}
								}
								else if matches.peek().is_some() {
									let next = matches.peek().expect("Failed to extract from non-empty iterator").clone();
									if getlineitem(&next.0) > (i, j) {
										break;
									}
									else if getlineitem(&next.1) > (i, j) {
										splitpairs.push((i, j, (next.0).2, s.chars().count()));
										on = true;
										break;
									}
									else {
										splitpairs.push((i, j, (next.0).2, (next.1).2));
										matches.next();
									}
								}
								else {
									break;
								}
							}
						}
					}
				}

				// Place the indices in a nested map for easy access later
				let mut matchmap: BTreeMap<usize, BTreeMap<usize, BTreeSet<(usize, usize)>>> = BTreeMap::new();
				for (line, item, start, end) in splitpairs {
					matchmap.entry(line).or_insert(BTreeMap::new()).entry(item).or_insert(BTreeSet::new()).insert((start, end));
				}
				matchmap
			},
		};

		Search { query: query.to_string(), matches: matchmap }
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
			if let Some(Output::Str(ref mut s)) = target.last_mut() { s.push(c); return; } // FIXME Using early return to avoid borrow-checker issues with an if-else representation
			target.push(Output::Str(c.to_string()));
		};
		let append = |target: &mut Vec<Vec<Output>>, mut content: Vec<Vec<Output>>| {
			if content.len() == 0 { }
			else if target.len() == 0 { target.append(&mut content); }
			else {
				target.last_mut().expect("Couldn't get last element from non-empty vector").append(&mut content[0]);
				target.extend(content.into_iter().skip(1));
			}
		};
		let strappend = |target: &mut Vec<String>, mut content: Vec<String>| {
			if content.len() == 0 {}
			else if target.len() == 0 { target.append(&mut content); }
			else {
				target.last_mut().expect("Couldn't get last element from non-empty vector").push_str(&mut content[0]);
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
						'\n' => {
							addchar(&mut cur, ' ');
							newline(output, &mut cur, &mut cnt, &mut need_mapping);
						},
						'\t' => {
							if output.width > 0 && cnt >= output.width - TABWIDTH {
								newline(output, &mut cur, &mut cnt, &mut need_mapping);
							}
							cur.push(Output::Str(std::iter::repeat(" ").take(TABWIDTH).collect::<String>())); // TODO What if width < 4?
							cnt += TABWIDTH;
							need_mapping = true;
						},
						c => {
							let cw = wcwidth::char_width(c).unwrap_or(0) as usize;
							if output.width > 0 && cnt + cw > output.width {
								newline(output, &mut cur, &mut cnt, &mut need_mapping);
							}
							addchar(&mut cur, c);
							cnt += cw;
						},
					}
					if record {
						output.raw.last_mut().expect("Found a preformatted with an empty raw").push(c);
						if need_mapping && cnt > 0 {
							let mut add_mapping = |charlen: usize, offset: usize| {
								let line = std::cmp::max(output.content.len() as isize - 1, 0) as usize;
								let item = output.content.last().map(|x| x.len()).unwrap_or(0) + cur.len() - 1;
								let idx = match cur.last() {
									Some(Output::Str(s)) => s.len() - charlen,
									_ => 0
								};
								// Note that the mapping is based on byte indices, not char
								output.mapping.insert((output.raw.len() - 1, output.raw.last().expect("Found a preformatted with an empty raw").len() - offset), (line, item, idx));
							};
							if c as i32 == 9 {
								add_mapping(TABWIDTH, 1);
								add_mapping(0, 0); // Only necessary for tabs at end of line
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
						let rawstart = (output.raw.len() - 1, output.raw.last().expect("Found a preformatted with an empty raw").len());
						let valstart = match output.content.last() {
							None => (0, 0, 0),
							Some(outlast) => (output.content.len() - 1, outlast.len(), 0),
						};
						for (k, v) in sub.mapping {
							let key = (k.0 + rawstart.0, if k.0 == 0 { k.1 + rawstart.1 } else { k.1 });
							let val = (v.0 + valstart.0, if v.0 == 0 { v.1 + valstart.1 } else { v.1 }, 0);
							output.mapping.insert(key, val);
						}
						strappend(&mut output.raw, sub.raw);
						if output.width == 0 || sublen <= output.width - startcol {
							append(&mut output.content, sub.content);
							startcol + sublen
						}
						else {
							assert!(sublen < output.width); // FIXME What should we do if the screen is narrower than a nowrap?
							output.content.append(&mut sub.content);
							sublen
						}
					},
					_ => panic!("Breaks not allowed in nobreak environment"), // TODO Support hard wraps in nobreak
				}
			},
			FmtCmd::Exclude(child) => {
				Self::internal_format(output, child, startcol, color, color_offset, false)
			},
		}
	}

	pub fn format(&self, width: usize, color_offset: usize) -> Preformatted {
		let mut ret = Preformatted::new(width);
		Self::internal_format(&mut ret, self, 0, 0 /* FIXME */, color_offset, true);
		/*eprintln!("RAW");
		ret.raw.iter().enumerate().for_each(|(i, x)| eprintln!("\t{}: {:?}", i, x));
		eprintln!("CONTENT");
		ret.content.iter().enumerate().for_each(|(i, x)| {
			x.iter().enumerate().for_each(|(j, y)| {
				if let Output::Str(s) = y {
					eprintln!("\t{}.{}: {:?}", i, j, s);
				}
			});
		});
		eprintln!("MAPPING");
		ret.mapping.iter().for_each(|(k, v)| eprintln!("\t{:?} -> {:?}", k, v));
		eprintln!("====");*/
		ret
	}

	pub fn contains(&self, q: &str) -> bool { // Search a value without having to preformat it
		match self {
			FmtCmd::Literal(value) => value.contains(q),
			FmtCmd::Container(children) => children.iter().any(|x| x.contains(q)),
			FmtCmd::Color(_, child) => child.contains(q),
			FmtCmd::NoBreak(child) => child.contains(q),
			FmtCmd::Exclude(_) => false,
		}
	}
}
