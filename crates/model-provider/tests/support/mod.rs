//! A tiny GBNF matcher for tests: enough of the syntax that `to_gbnf` writes, nothing more.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
enum Node {
    Lit(Vec<char>),
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Rule(String),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Rep(Box<Node>, u32, Option<u32>),
}

pub struct Grammar {
    rules: BTreeMap<String, Node>,
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    _text: &'a str,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(|c| c == ' ') {
            self.at += 1;
        }
    }
    fn escape(&mut self) -> char {
        let c = self.peek().expect("an escape");
        self.at += 1;
        match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'x' | 'u' => {
                let width = if c == 'x' { 2 } else { 4 };
                let hex: String = self.chars[self.at..self.at + width].iter().collect();
                self.at += width;
                char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap()
            }
            other => other,
        }
    }
    fn alt(&mut self) -> Node {
        let mut alts = vec![self.seq()];
        loop {
            self.ws();
            if self.peek() == Some('|') {
                self.at += 1;
                alts.push(self.seq());
            } else {
                return Node::Alt(alts);
            }
        }
    }
    fn seq(&mut self) -> Node {
        let mut items = Vec::new();
        loop {
            self.ws();
            match self.peek() {
                None | Some('|') | Some(')') => return Node::Seq(items),
                _ => items.push(self.item()),
            }
        }
    }
    fn item(&mut self) -> Node {
        let mut node = match self.peek().unwrap() {
            '"' => {
                self.at += 1;
                let mut text = Vec::new();
                while self.peek() != Some('"') {
                    let c = self.peek().unwrap();
                    self.at += 1;
                    text.push(if c == '\\' { self.escape() } else { c });
                }
                self.at += 1;
                Node::Lit(text)
            }
            '[' => {
                self.at += 1;
                let negated = self.peek() == Some('^');
                if negated {
                    self.at += 1;
                }
                let mut ranges = Vec::new();
                while self.peek() != Some(']') {
                    let lo = self.class_char();
                    if self.peek() == Some('-') && self.chars.get(self.at + 1) != Some(&']') {
                        self.at += 1;
                        ranges.push((lo, self.class_char()));
                    } else {
                        ranges.push((lo, lo));
                    }
                }
                self.at += 1;
                Node::Class { negated, ranges }
            }
            '(' => {
                self.at += 1;
                let inner = self.alt();
                self.ws();
                assert_eq!(self.peek(), Some(')'));
                self.at += 1;
                inner
            }
            _ => {
                let start = self.at;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    self.at += 1;
                }
                Node::Rule(self.chars[start..self.at].iter().collect())
            }
        };
        loop {
            node = match self.peek() {
                Some('*') => Node::Rep(Box::new(node), 0, None),
                Some('+') => Node::Rep(Box::new(node), 1, None),
                Some('?') => Node::Rep(Box::new(node), 0, Some(1)),
                Some('{') => {
                    let close = self.chars[self.at..]
                        .iter()
                        .position(|c| *c == '}')
                        .unwrap();
                    let body: String = self.chars[self.at + 1..self.at + close].iter().collect();
                    self.at += close;
                    let (lo, hi) = match body.split_once(',') {
                        None => (body.parse().unwrap(), Some(body.parse().unwrap())),
                        Some((lo, "")) => (lo.parse().unwrap(), None),
                        Some((lo, hi)) => (lo.parse().unwrap(), Some(hi.parse().unwrap())),
                    };
                    Node::Rep(Box::new(node), lo, hi)
                }
                _ => return node,
            };
            self.at += 1;
        }
    }
    fn class_char(&mut self) -> char {
        let c = self.peek().unwrap();
        self.at += 1;
        if c == '\\' { self.escape() } else { c }
    }
}

impl Grammar {
    pub fn parse(text: &str) -> Grammar {
        let rules = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                let (name, body) = line.split_once(" ::= ").expect("a rule line");
                let mut parser = Parser {
                    chars: body.chars().collect(),
                    at: 0,
                    _text: body,
                };
                (name.to_owned(), parser.alt())
            })
            .collect();
        Grammar { rules }
    }

    pub fn accepts(&self, input: &str) -> bool {
        let chars: Vec<char> = input.chars().collect();
        let root = Node::Rule("root".into());
        self.ends(&root, &chars, 0).contains(&chars.len())
    }

    fn ends(&self, node: &Node, input: &[char], at: usize) -> BTreeSet<usize> {
        match node {
            Node::Lit(text) => (input.len() >= at + text.len()
                && input[at..at + text.len()] == text[..])
                .then_some(at + text.len())
                .into_iter()
                .collect(),
            Node::Class { negated, ranges } => input
                .get(at)
                .filter(|c| ranges.iter().any(|(lo, hi)| (lo..=hi).contains(c)) != *negated)
                .map(|_| at + 1)
                .into_iter()
                .collect(),
            Node::Rule(name) => self.ends(&self.rules[name], input, at),
            Node::Seq(items) => items.iter().fold(BTreeSet::from([at]), |starts, item| {
                starts
                    .iter()
                    .flat_map(|s| self.ends(item, input, *s))
                    .collect()
            }),
            Node::Alt(alts) => alts.iter().flat_map(|a| self.ends(a, input, at)).collect(),
            Node::Rep(inner, min, max) => {
                let mut result = BTreeSet::new();
                let mut frontier = BTreeSet::from([at]);
                let mut count = 0;
                loop {
                    if count >= *min {
                        result.extend(frontier.iter().copied());
                    }
                    if max.is_some_and(|m| count >= m) || frontier.is_empty() {
                        return result;
                    }
                    let next: BTreeSet<usize> = frontier
                        .iter()
                        .flat_map(|s| self.ends(inner, input, *s))
                        .collect();
                    if count >= *min && next.is_subset(&frontier) {
                        return result;
                    }
                    frontier = next;
                    count += 1;
                }
            }
        }
    }
}
