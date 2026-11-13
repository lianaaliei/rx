# rx

موتور Regex از صفر در Rust، بدون وابستگی: **parser → کامپایلر بایت‌کد → Pike VM**

- زمان تطبیق `O(طول متن × اندازه الگو)`؛ هیچ الگویی (مثل `(a*)*b`) باعث backtracking نمایی نمی‌شود.
- semantics از نوع Perl (leftmost-first) با گروه‌های capture، گروه‌های نام‌دار، حریص/تنبل.
- پشتیبانی: `. [a-z] [^..] \d \w \s \D \W \S \b \B ^ $ ( ) (?: ) (?P<n> ) | * + ? {n,m}` و Unicode.
- عمداً بدون backreference و lookaround (این‌ها زمان خطی را از بین می‌برند).
- محدودیت عمق تو در تو و اندازه برنامه (در برابر ورودی مخرب).

## استفاده
    use rx::Regex;
    let re = Regex::new(r"(?P<y>\d{4})-(?P<m>\d{2})").unwrap();
    let c = re.captures("2026-11").unwrap();
    assert_eq!(c.name("m").unwrap().as_str(), "11");
    re.replace_all("a=1", "${v}=$k");

## CLI شبیه grep
    rx -n 'p(a|e)ttern' file.txt
    rx -o '\d+' file.txt
    rx --dump '(a|b)*c'      # نمایش بایت‌کد

## تست
    cargo test --release
شامل fuzz تفاضلی با یک matcher مرجع (۱۰هزار+ ترکیب)، تست خطاهای سینتکس و تست الگوهای فاجعه‌بار.
