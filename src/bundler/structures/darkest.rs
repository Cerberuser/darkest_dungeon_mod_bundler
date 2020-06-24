use super::BTreeMappable;

#[derive(Clone, Debug, Default)]
struct DarkestEntry(Vec<(String, Vec<String>)>);

#[derive(Clone, Debug, Default)]
struct DarkestFile(Vec<(String, DarkestEntry)>);

macro_rules! explode {
    ($with:ident) => {
        |(key, value)| {
            value
                .0
                .$with()
                .map(move |(subkey, value)| ((key.clone(), subkey.clone()), value))
        }
    };
}

impl super::MapPath for (String, String) {}

impl BTreeMappable for DarkestFile {
    type Key = (String, String);
    type Value = Vec<String>;
    fn map(&self) -> std::collections::BTreeMap<Self::Key, &Self::Value> {
        self.0.iter().flat_map(explode!(iter)).collect()
    }

    fn map_mut(&mut self) -> std::collections::BTreeMap<Self::Key, &mut Self::Value> {
        self.0.iter_mut().flat_map(explode!(iter_mut)).collect()
    }

    fn clone_with(
        &self,
        f: impl FnOnce(&mut std::collections::BTreeMap<Self::Key, Self::Value>),
    ) -> Self {
        let mut map = self
            .0
            .iter()
            .cloned()
            .flat_map(explode!(into_iter))
            .collect();
        f(&mut map);
        let mut v = vec![];
        let mut last = None;
        for ((key, subkey), value) in map {
            if last.as_ref() != Some(&key) {
                last = Some(key.clone());
                v.push((key, DarkestEntry(vec![])))
            }
            (v.last_mut().unwrap().1).0.push((subkey, value));
        }
        Self(v)
    }
}

mod parser {
    use super::{DarkestEntry, DarkestFile};
    use combine::{
        choice, eof, many, many1, one_of, optional,
        parser::{
            char::{alpha_num, char as exact_char, digit, letter, space},
            repeat::{skip_many, skip_many1, skip_until, take_until},
        },
        sep_by1, ParseError, ParseResult, Parser, Stream, StreamOnce, not_followed_by,
    };
    use std::marker::PhantomData;

    fn key_value<Input, Key, Value>(
        key: impl Parser<Input, Output = Key>,
        value: impl Parser<Input, Output = Value>,
    ) -> impl Parser<Input, Output = (Key, Value)>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        (key, skip_many1(space()), value).map(|(key, (), value)| (key, value))
    }

    fn comment<Input>() -> impl Parser<Input, Output = ()>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        (
            exact_char('/'),
            skip_until(one_of("\r\n".chars())),
            skip_many1(one_of("\r\n".chars())),
        )
            .map(|_| {})
    }

    macro_rules! parse_and_do {
        ($input:ident with $parser:expr => $then:expr) => {{
            match optional($parser).parse($input) {
                Ok((ret, rest)) => {
                    $input = rest;
                    if let Some(_) = ret {
                        $then;
                    }
                }
                Err(err) => {
                    return ParseResult::CommitErr(err);
                }
            }
        }};
    }

    macro_rules! parse_and_discard {
        ($input:ident with $parser:expr) => {
            $input = match $parser.parse($input) {
                Ok((_, input)) => input,
                Err(err) => return ParseResult::CommitErr(err),
            };
        };
    }

    struct ItemsParser<Input>(PhantomData<Input>);
    impl<Input> ItemsParser<Input> {
        fn new() -> Self {
            Self(PhantomData)
        }
    }

    enum ItemStepResult {
        Key(String),
        Value(String),
    }

    impl<Input> Parser<Input> for ItemsParser<Input>
    where
        Input: Stream<Token = char>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    {
        type Output = Vec<(String, Vec<String>)>;
        type PartialState = ();

        fn parse_lazy(
            &mut self,
            mut input: &mut Input,
        ) -> ParseResult<Self::Output, <Input as StreamOnce>::Error> {
            let mut output = vec![];
            let mut cur_item = None;
            loop {
                // First of all, skip every whitespace, including newlines, and any possible comments.
                let skipped = choice((one_of(" \t\r\n".chars()).map(|_| {}), comment()));
                parse_and_discard!(input with skip_many(skipped));
                // If we're at the end of input - we're done.
                parse_and_do!(input with eof() => break);
                // If we can parse the next entry - we're also done.
                // TODO: find more idiomatic way!
                if let Err(_) = not_followed_by(DarkestEntry::key().map(|_| "next")).parse(&mut *input) {
                    break;
                }
                // Now, we should try to get the next item.
                // It might be either the key or the value.
                match choice((
                    DarkestEntry::subkey().map(ItemStepResult::Key),
                    DarkestEntry::value().map(ItemStepResult::Value),
                ))
                .parse(input)
                {
                    Ok((item, rest)) => {
                        input = rest;
                        match item {
                            // Next key - pushing the collected result for the previous key, if any.
                            ItemStepResult::Key(key) => {
                                if let Some((old_key, v)) = cur_item.replace((key, vec![])) {
                                    output.push((old_key, v));
                                }
                            }
                            // Next value - push it into the result currently being collected.
                            ItemStepResult::Value(value) => match &mut cur_item {
                                Some((_, v)) => v.push(value),
                                None => {
                                    // If there's no such result, it means that the value came before the key.
                                    let mut err =
                                        <Input as StreamOnce>::Error::empty(input.position());
                                    err.add_expected("key");
                                    err.add_unexpected("value");
                                    return ParseResult::CommitErr(err);
                                }
                            },
                        }
                    }
                    Err(err) => return ParseResult::CommitErr(err),
                };
            }

            if let Some((old_key, v)) = cur_item.take() {
                output.push((old_key, v));
                ParseResult::CommitOk(output)
            } else {
                let mut err = <Input as StreamOnce>::Error::empty(input.position());
                err.add_expected("key-value pair");
                ParseResult::CommitErr(err)
            }
        }
    }

    impl DarkestEntry {
        fn key<Input>() -> impl Parser<Input, Output = String>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            (Self::ident(), exact_char(':')).map(|(ident, _)| ident)
        }

        fn subkey<Input>() -> impl Parser<Input, Output = String>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            (exact_char('.'), Self::ident()).map(|(_, ident)| ident)
        }

        fn ident<Input>() -> impl Parser<Input, Output = String>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            let in_ident = || (alpha_num(), exact_char('_'));

            (letter(), many(choice(in_ident())))
                .map(|(first, rest): (char, String)| format!("{}{}", first, rest))
        }

        fn value<Input>() -> impl Parser<Input, Output = String>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            let quoted_string = (
                exact_char('"'),
                take_until(exact_char('"')),
                exact_char('"'),
            )
                .map(|(_, s, _)| s);
            let number = || {
                (
                    optional(exact_char('-')),
                    many1(digit()),
                    optional(exact_char('.').with(many1(digit()))),
                    optional(exact_char('%')),
                )
                    .map(|(minus, first, second, percent): (_, String, Option<String>, _)| {
                        let minus = minus.map(|c: char| c.to_string()).unwrap_or("".into());
                        let second = second.map(|second| format!(".{}", second)).unwrap_or("".into());
                        let percent = percent.map(|c: char| c.to_string()).unwrap_or("".into());
                        format!("{}{}{}{}", minus, first, second, percent)
                    })
            };
            choice((Self::ident(), quoted_string, number()))
        }

        fn parser<Input>() -> impl Parser<Input, Output = (String, Self)>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            let pairs = ItemsParser::<Input>::new().map(Self);

            key_value(
                Self::key().message("Key parser failed in entry parser"),
                pairs.message("Pairs parser failed in entry parser"),
            )
        }
    }

    impl DarkestFile {
        pub fn parser<Input>() -> impl Parser<Input, Output = Self>
        where
            Input: Stream<Token = char>,
            Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
        {
            let skipped = || choice((one_of(" \t\r\n".chars()).map(|_| {}), comment()));
            skip_many(skipped())
                .with(sep_by1(
                    DarkestEntry::parser().message("Entry parser failed in file parser"),
                    skip_many(skipped()),
                ))
                .map(Self)
        }
    }

    #[cfg(test)]
    mod test {
        use super::{DarkestEntry, DarkestFile, ItemsParser};
        use combine::{easy::Errors, stream::PointerOffset, EasyParser};

        fn bail(err: Errors<char, &str, PointerOffset<str>>, source: &str) -> ! {
            let pos = err.position.translate_position(source);
            let mut err = err.map_position(|_| pos);
            err.add_error(combine::easy::Error::Message(combine::easy::Info::Owned(
                "Context: ..."
                    .chars()
                    .chain(source.chars().skip(pos.saturating_sub(10)).take(20))
                    .chain("...".chars())
                    .collect(),
            )));
            panic!("{}", err);
        }

        #[test]
        fn parse_values() {
            for s in &["value", "\"value1 value2\"", "123.45", "123.45%"] {
                DarkestEntry::value()
                    .easy_parse(*s)
                    .unwrap_or_else(|err| bail(err, s));
            }
        }

        #[test]
        fn parse_item() {
            let slice = ".key value \"value1 value2\"  123.45% 123.45";
            ItemsParser::new()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
        }

        #[test]
        fn parse_items() {
            let slice = ".key value .key2 value2";
            let (items, rest) = ItemsParser::new()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
            assert_eq!(rest, "");
            assert_eq!(items.len(), 2);
        }

        #[test]
        fn parse_entry() {
            let slice = "key: .string value .strings value1 \"value2 value3\" .number 123.45 .percent 123.45%";
            DarkestEntry::parser()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
        }

        #[test]
        fn parse_single_entry_file() {
            let slice = "key: .string value .strings value1 \"value2 value3\" .number 123.45 .percent 123.45%";
            DarkestFile::parser()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
        }

        #[test]
        fn parse_double_entry_file() {
            let slice = "
            key: .string value .strings value1 \"value2 value3\" .number 123.45 .percent 123.45%
            
            key2: .single value
            ";
            DarkestFile::parser()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
        }

        #[test]
        fn parse_complex_file() {
            let slice = include_str!("base.effects.darkest");
            DarkestFile::parser()
                .easy_parse(slice)
                .unwrap_or_else(|err| bail(err, slice));
        }
    }
}
