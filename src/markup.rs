use regex::Regex;

/// Оборачивает [spoiler]...[/spoiler] в спан.
fn spoiler_re() -> Regex {
    Regex::new(r"(?is)\[spoiler\](.*?)\[/spoiler\]").unwrap()
}

/// Превращает >>123456 в ссылку на пост.
fn quote_re() -> Regex {
    Regex::new(r"(?i)&gt;&gt;(\d{1,10})").unwrap()
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Рендерит тело поста в HTML.
///
/// Порядок: экранирование -> спойлеры -> цитаты -> greentext (построчно).
/// Результат предназначен для вывода с фильтром `|safe`.
pub fn render_body(text: &str) -> String {
    let escaped = escape_html(text);

    let with_spoilers = spoiler_re().replace_all(
        &escaped,
        "<span class=\"spoiler\">$1</span>",
    );

    let with_quotes = quote_re().replace_all(
        &with_spoilers,
        "<a href=\"#p$1\" class=\"ref\" data-ref=\"$1\">&gt;&gt;$1</a>",
    );

    // Greentext: строка начинается с ">" (но не ">>").
    let mut out = String::with_capacity(with_quotes.len() + 64);
    for line in with_quotes.lines() {
        if line.starts_with("&gt;") && !line.starts_with("&gt;&gt;") {
            out.push_str("<span class=\"quote\">");
            out.push_str(line);
            out.push_str("</span>\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        let out = render_body("<script>alert('x')</script> & stuff");
        assert!(!out.contains("<script>"));
        assert!(out.contains("&lt;script&gt;"));
    }

    #[test]
    fn greentext_line() {
        let out = render_body(">be me\nnormal line");
        assert!(out.contains("<span class=\"quote\">&gt;be me</span>"));
    }

    #[test]
    fn quote_link() {
        let out = render_body("see >>123456");
        assert!(out.contains("<a href=\"#p123456\" class=\"ref\" data-ref=\"123456\">&gt;&gt;123456</a>"));
    }

    #[test]
    fn spoiler() {
        let out = render_body("secret [spoiler]hidden[/spoiler] text");
        assert!(out.contains("<span class=\"spoiler\">hidden</span>"));
    }

    #[test]
    fn no_greentext_for_quote() {
        let out = render_body(">>123456");
        assert!(!out.contains("class=\"quote\""));
        assert!(out.contains("&gt;&gt;123456</a>"));
    }
}
