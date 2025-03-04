use std::{borrow::Cow, collections::VecDeque, fmt, mem::take, time::Duration};

use anyhow::{Error, bail};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Line<'a> {
    Comment { text: &'a str },
    Field { name: &'a str, value: &'a str },
    Empty,
}

impl<'a> Line<'a> {
    /// `line` must not contain '\r' or '\n'.
    fn from_str(line: &'a str) -> Self {
        match line.split_once(":") {
            Some((name, value)) => {
                if !name.is_empty() {
                    Self::Field {
                        name,
                        value: value.strip_prefix(' ').unwrap_or(value),
                    }
                } else {
                    Self::Comment { text: value }
                }
            }
            None => {
                if !line.is_empty() {
                    Self::Field {
                        name: line,
                        value: "",
                    }
                } else {
                    Self::Empty
                }
            }
        }
    }

    fn write_to(&self, out: &mut impl fmt::Write) -> Result<(), fmt::Error> {
        match self {
            Self::Comment { text } => {
                out.write_char(':')?;
                out.write_str(text)?;
            }
            Self::Field { name, value } => {
                out.write_str(name)?;
                if !value.is_empty() {
                    out.write_str(": ")?;
                    out.write_str(value)?;
                }
            }
            Self::Empty => (),
        }
        Ok(())
    }
}

#[derive(Clone, Default, PartialEq, Eq, Hash, Debug)]
pub struct Event<'a> {
    pub type_: Option<Cow<'a, str>>,
    pub data: Option<Cow<'a, str>>,
    pub id: Option<Cow<'a, str>>,
    pub retry: Option<Duration>,
}

impl<'a> Event<'a> {
    pub fn update_field(&mut self, name: &'a str, value: &'a str) {
        match name {
            "event" => {
                self.type_ = Some(Cow::Borrowed(value));
            }
            "data" => {
                self.data = Some(match self.data.take() {
                    Some(data) => {
                        let mut data = data.into_owned();
                        data.push('\n');
                        data.push_str(value);
                        Cow::Owned(data)
                    }
                    None => Cow::Borrowed(value),
                });
            }
            "id" => {
                self.id = Some(Cow::Borrowed(value));
            }
            "retry" => {
                if let Ok(ms) = value.parse::<u64>() {
                    self.retry = Some(Duration::from_millis(ms));
                }
            }
            _ => (),
        }
    }

    pub fn write_to(&self, out: &mut impl fmt::Write) -> Result<(), fmt::Error> {
        let lines = (self.type_.iter().map(|value| Line::Field {
            name: "event",
            value,
        }))
        .chain(
            self.data
                .iter()
                .flat_map(|value| value.split('\n'))
                .map(|value| Line::Field {
                    name: "data",
                    value,
                }),
        )
        .chain(
            self.id
                .iter()
                .map(|value| Line::Field { name: "id", value }),
        );
        for line in lines {
            line.write_to(out)?;
            out.write_char('\n')?;
        }

        if let Some(retry) = &self.retry {
            writeln!(out, "retry: {}", retry.as_millis())?;
        }

        out.write_char('\n')?;
        Ok(())
    }
}

impl Event<'_> {
    fn into_owned(self) -> Event<'static> {
        Event {
            type_: self.type_.map(|s| Cow::Owned(s.into_owned())),
            data: self.data.map(|s| Cow::Owned(s.into_owned())),
            id: self.id.map(|s| Cow::Owned(s.into_owned())),
            retry: self.retry,
        }
    }
}

struct EventIterator<'a> {
    event: Event<'a>,
    data: &'a str,
}

impl<'a> Iterator for EventIterator<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(i) = self.data.find(['\r', '\n']) {
            let (line, mut remaining) = self.data.split_at(i);
            if self.data.get(i..(i + 2)) == Some("\r\n") {
                remaining = &remaining[2..];
            } else {
                remaining = &remaining[1..];
            }
            self.data = remaining;
            match Line::from_str(line) {
                Line::Comment { .. } => (),
                Line::Field { name, value } => self.event.update_field(name, value),
                Line::Empty => {
                    if self.event != Event::default() {
                        return Some(take(&mut self.event));
                    }
                }
            }
        }
        None
    }
}

#[derive(Default)]
pub struct EventReader {
    event: Event<'static>,
    prefix: Vec<u8>,
    suffix: VecDeque<u8>,
}

pub struct EventReaderIterator<'a> {
    event: &'a mut Event<'static>,
    suffix: &'a mut VecDeque<u8>,
    iter: EventIterator<'a>,
}

impl EventReader {
    pub fn next_events<'a>(
        &'a mut self,
        bytes: &'a [u8],
    ) -> Result<EventReaderIterator<'a>, Error> {
        self.prefix.clear();
        self.prefix.extend(self.suffix.drain(..));

        let bytes = if self.prefix.is_empty() {
            bytes
        } else {
            self.prefix.extend_from_slice(bytes);
            &self.prefix
        };

        let data = if let Some(chunk) = bytes.utf8_chunks().next() {
            if chunk.valid().len() + chunk.invalid().len() != bytes.len() {
                bail!("Bytes contain invalid UTF8");
            }
            self.suffix.extend(chunk.invalid());
            chunk.valid()
        } else {
            ""
        };

        let event = take(&mut self.event);
        Ok(EventReaderIterator {
            event: &mut self.event,
            suffix: &mut self.suffix,
            iter: EventIterator { event, data },
        })
    }
}

impl<'a> Iterator for EventReaderIterator<'a> {
    type Item = Event<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl Drop for EventReaderIterator<'_> {
    fn drop(&mut self) {
        *self.event = take(&mut self.iter.event).into_owned();
        for b in self.iter.data.as_bytes().iter().rev() {
            self.suffix.push_front(*b);
        }
    }
}

#[test]
fn data() {
    let data = &"
data

data
data

data:
"[1..];

    let mut iter = EventIterator {
        event: Event::default(),
        data,
    };

    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("".into()),
            ..Default::default()
        }
    );
    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("\n".into()),
            ..Default::default()
        }
    );
    assert!(iter.next().is_none())
}

#[test]
fn comment_and_id() {
    let data = &"
: test stream

data: first event
id: 1

data:second event
id

data:  third event
"[1..];

    let mut iter = EventIterator {
        event: Event::default(),
        data,
    };

    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("first event".into()),
            id: Some("1".into()),
            ..Default::default()
        }
    );
    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("second event".into()),
            id: Some("".into()),
            ..Default::default()
        }
    );
    assert!(iter.next().is_none())
}

#[test]
fn multiline_data() {
    let data = &"
data: YHOO
data: +2
data: 10

"[1..];

    let mut iter = EventIterator {
        event: Event::default(),
        data,
    };

    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("YHOO\n+2\n10".into()),
            ..Default::default()
        }
    );
    assert!(iter.next().is_none())
}

#[test]
fn split_event() {
    let chunks = [b"event: push\ndata", b": 123456\nid: 1\n\n"];

    let mut reader = EventReader::default();

    assert!(reader.next_events(chunks[0]).unwrap().next().is_none());

    let mut iter = reader.next_events(chunks[1]).unwrap();
    assert_eq!(
        iter.next().unwrap(),
        Event {
            type_: Some("push".into()),
            data: Some("123456".into()),
            id: Some("1".into()),
            ..Default::default()
        }
    );
    assert!(iter.next().is_none())
}

#[test]
fn split_char() {
    let chunks = [&b"data: \xd0\x90\xd0"[..], &b"\x91\xd0\x92\n\n"[..]];

    let mut reader = EventReader::default();

    assert!(reader.next_events(chunks[0]).unwrap().next().is_none());

    let mut iter = reader.next_events(chunks[1]).unwrap();
    assert_eq!(
        iter.next().unwrap(),
        Event {
            data: Some("АБВ".into()),
            ..Default::default()
        }
    );
    assert!(iter.next().is_none())
}
