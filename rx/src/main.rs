use rx::Regex;
use std::io::{self, BufRead};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |args: &mut Vec<String>, f: &str| args.iter().position(|a| a == f).map(|i| args.remove(i)).is_some();
    let (only, number, dump) = (flag(&mut args, "-o"), flag(&mut args, "-n"), flag(&mut args, "--dump"));
    let Some(pattern) = args.first() else {
        eprintln!("usage: rx [-n] [-o] [--dump] PATTERN [FILE]   (reads stdin without FILE)");
        std::process::exit(2);
    };
    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if dump {
        print!("{}", re.dump());
        return;
    }
    let reader: Box<dyn BufRead> = match args.get(1) {
        Some(f) => Box::new(io::BufReader::new(std::fs::File::open(f).unwrap_or_else(|e| {
            eprintln!("{f}: {e}");
            std::process::exit(2)
        }))),
        None => Box::new(io::stdin().lock()),
    };
    let mut found = false;
    for (i, line) in reader.lines().enumerate() {
        let Ok(line) = line else { break };
        let ms = re.find_all(&line);
        if ms.is_empty() {
            continue;
        }
        found = true;
        let prefix = if number { format!("{}:", i + 1) } else { String::new() };
        if only {
            ms.iter().filter(|m| m.start != m.end).for_each(|m| println!("{prefix}{}", m.as_str()));
        } else {
            println!("{prefix}{line}");
        }
    }
    std::process::exit(if found { 0 } else { 1 });
}
