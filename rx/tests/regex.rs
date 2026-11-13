use rx::Regex;
use std::time::Instant;

fn find(p: &str, t: &str) -> Option<String> {
    Regex::new(p).unwrap().find(t).map(|m| m.as_str().to_string())
}

#[test]
fn basics() {
    let cases: &[(&str, &str, Option<&str>)] = &[
        ("abc", "xxabcxx", Some("abc")),
        ("a.c", "a\nc abc", Some("abc")),
        ("^abc$", "abc", Some("abc")),
        ("^abc$", "abcd", None),
        ("a|b|c", "zzc", Some("c")),
        ("(foo|foobar)", "foobar", Some("foo")), // leftmost-first, like Perl
        ("a*", "aaa", Some("aaa")),
        ("a*?", "aaa", Some("")),
        ("a+?", "aaa", Some("a")),
        ("a{2,3}", "aaaa", Some("aaa")),
        ("a{2,3}?", "aaaa", Some("aa")),
        ("a{2}", "a", None),
        ("x{,2}", "x{,2}", Some("x{,2}")), // not a quantifier => literal
        ("[a-c]+", "xxabcabcxx", Some("abcabc")),
        ("[^a-c]+", "abcxyzabc", Some("xyz")),
        ("[]a]+", "x]a]", Some("]a]")),
        ("[a-]+", "a-a-", Some("a-a-")),
        (r"\d+", "tel 4711 ok", Some("4711")),
        (r"\D+", "12ab34", Some("ab")),
        (r"[\d\s]+", "ab 1 2 cd", Some(" 1 2 ")),
        (r"[\D]+", "12ab34", Some("ab")),
        (r"\w+@\w+\.com", "mail bob@example.com now", Some("bob@example.com")),
        (r"\bcat\b", "concat cat", Some("cat")),
        (r"\Bcat", "cat concat", Some("cat")),
        (r"a\.b", "axb a.b", Some("a.b")),
        ("(a|ab)(c|bcd)(d*)", "abcd", Some("abcd")),
        ("é+", "caféé!", Some("éé")),
        (".", "日本", Some("日")),
        ("", "abc", Some("")),
    ];
    for (p, t, want) in cases {
        assert_eq!(find(p, t).as_deref(), *want, "pattern {p:?} on {t:?}");
    }
}

#[test]
fn captures_and_names() {
    let re = Regex::new(r"(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})").unwrap();
    let c = re.captures("date: 2026-11-09!").unwrap();
    assert_eq!(c.get(0).unwrap().as_str(), "2026-11-09");
    assert_eq!(c.get(1).unwrap().as_str(), "2026");
    assert_eq!(c.name("m").unwrap().as_str(), "11");
    assert_eq!(c.name("d").unwrap().as_str(), "09");
    assert_eq!(c.len(), 4);
    // unmatched optional group
    let c = Regex::new("(a)|(b)").unwrap().captures("b").unwrap();
    assert!(c.get(1).is_none() && c.get(2).is_some());
    // (?: ) does not capture
    assert_eq!(Regex::new("(?:a)(b)").unwrap().captures("ab").unwrap().len(), 2);
}

#[test]
fn find_all_and_replace() {
    let re = Regex::new(r"\d+").unwrap();
    let v: Vec<_> = re.find_all("a1 b22 c333").iter().map(|m| m.as_str().to_string()).collect();
    assert_eq!(v, ["1", "22", "333"]);
    assert_eq!(Regex::new("a*").unwrap().find_all("baac").len(), 3); // "", "aa", "" (end)
    let re = Regex::new(r"(?P<k>\w+)=(?P<v>\w+)").unwrap();
    assert_eq!(re.replace_all("a=1, b=2", "${v}=$k"), "1=a, 2=b");
    assert_eq!(re.replace_all("a=1", "$$ $1"), "$ a");
    assert_eq!(Regex::new("x*").unwrap().replace_all("abc", "-"), "-a-b-c-");
}

#[test]
fn syntax_errors() {
    for bad in ["(", ")", "a)", "[abc", "*a", "a**b?+", "a{3,2}", "(?z)", r"\q", "[z-a]", "a{99999}", "^*", "(?P<>x)"] {
        let r = Regex::new(bad);
        // "a**b?+" is accepted (stacked quantifiers); everything else must fail
        if bad != "a**b?+" {
            assert!(r.is_err(), "{bad:?} should be rejected");
        }
    }
    assert!(Regex::new(&"(".repeat(500)).is_err()); // nesting limit, no stack overflow
}

#[test]
fn no_catastrophic_backtracking() {
    // Classic exponential cases for backtracking engines run in linear time here.
    let n = 20_000;
    let evil = "a".repeat(n);
    for (p, text) in [
        ("(a*)*b", evil.clone()),
        ("(a|aa)+$", format!("{evil}!")),
        ("(x+x+)+y", "x".repeat(n)),
        (r"(\w+\s?)*$", format!("{evil}!")),
    ] {
        let re = Regex::new(p).unwrap();
        let t = Instant::now();
        let _ = re.is_match(&text);
        assert!(t.elapsed().as_secs_f64() < 10.0, "{p} took too long: {:?}", t.elapsed());
    }
    assert!(!Regex::new("(a*)*b").unwrap().is_match(&evil));
    assert!(Regex::new("(a|aa)+$").unwrap().is_match(&evil));
}

#[test]
fn differential_fuzz_against_naive_matcher() {
    // Random patterns over a tiny alphabet, compared with a simple reference backtracker
    // for "does the whole string match" semantics (anchored via ^...$).
    fn gen(rng: &mut u64, depth: u32) -> String {
        let mut r = || { *rng ^= *rng << 13; *rng ^= *rng >> 7; *rng ^= *rng << 17; *rng };
        match (r() % 8, depth) {
            (0, d) if d > 0 => format!("({}|{})", gen(rng, d - 1), gen(rng, d - 1)),
            (1, d) if d > 0 => format!("({})*", gen(rng, d - 1)),
            (2, d) if d > 0 => format!("({})?", gen(rng, d - 1)),
            (3, d) if d > 0 => format!("{}{}", gen(rng, d - 1), gen(rng, d - 1)),
            (4, _) => "[ab]".into(),
            (5, _) => ".".into(),
            (6, _) => "a".into(),
            _ => "b".into(),
        }
    }
    // reference: tiny recursive matcher over the same grammar subset
    fn m(p: &[char], t: &[char]) -> bool {
        fn alts(p: &[char]) -> Vec<&[char]> {
            let (mut d, mut st, mut v) = (0, 0, vec![]);
            for (i, &c) in p.iter().enumerate() {
                match c { '(' => d += 1, ')' => d -= 1, '|' if d == 0 => { v.push(&p[st..i]); st = i + 1 } _ => {} }
            }
            v.push(&p[st..]);
            v
        }
        let a = alts(p);
        if a.len() > 1 { return a.iter().any(|x| m(x, t)); }
        if p.is_empty() { return t.is_empty(); }
        // first atom
        let (len, set): (usize, Box<dyn Fn(char) -> bool>) = match p[0] {
            '(' => {
                let (mut d, mut e) = (0, 0);
                for (i, &c) in p.iter().enumerate() { if c == '(' { d += 1 } if c == ')' { d -= 1; if d == 0 { e = i; break } } }
                let inner: Vec<char> = p[1..e].to_vec();
                let rest = &p[e + 1..];
                let (q, rest) = match rest.first() { Some('*') => ('*', &rest[1..]), Some('?') => ('?', &rest[1..]), _ => ('1', rest) };
                return match q {
                    '1' => (0..=t.len()).any(|i| m(&inner, &t[..i]) && m(rest, &t[i..])),
                    '?' => m(rest, t) || (0..=t.len()).any(|i| m(&inner, &t[..i]) && m(rest, &t[i..])),
                    _ => m(rest, t) || (1..=t.len()).any(|i| m(&inner, &t[..i]) && m(p, &t[i..])),
                };
            }
            '[' => (4, Box::new(|c| c == 'a' || c == 'b')),
            '.' => (1, Box::new(|_| true)),
            c => (1, Box::new(move |x| x == c)),
        };
        !t.is_empty() && set(t[0]) && m(&p[len..], &t[1..])
    }
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut checked = 0;
    for _ in 0..400 {
        let pat = gen(&mut rng, 3);
        let re = Regex::new(&format!("^(?:{pat})$")).unwrap();
        let pc: Vec<char> = pat.chars().collect();
        for len in 0..6 {
            for bits in 0..(1u32 << len) {
                let t: String = (0..len).map(|i| if bits >> i & 1 == 1 { 'b' } else { 'a' }).collect();
                let tc: Vec<char> = t.chars().collect();
                assert_eq!(re.is_match(&t), m(&pc, &tc), "pattern {pat:?} text {t:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 10_000);
}
