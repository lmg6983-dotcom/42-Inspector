mod checks;
mod input;
mod spec;

use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "기재불비 검사기

사용법: inspector42 [--json] [--style | --dump-text | --sign-list] [명세서 파일]
  파일: TXT(UTF-8 · UTF-16 · CP949), DOCX, HWPX, HWP 5.0. 없으면 표준 입력.
  --json       결과를 JSON으로 (형식은 AGENTS.md)
  --style      문체 검사도 함께 (가운데 점 · 불릿 · 순서 표현 · 본문의 '청구항' · 과장 · 단정 표현)
  --dump-text  검사하지 않고, 파일에서 뽑은 텍스트만 출력 (줄 번호의 기준)
  --sign-list  검사하지 않고, 본문의 '이름(부호)'로 【부호의 설명】 블록을 만들어 출력
종료 코드: 0 지적 없음 · 1 지적 있음 · 2 사용법 오류 또는 읽기 실패";

fn main() -> ExitCode {
    let mut json = false;
    let mut dump = false;
    let mut style = false;
    let mut sign_list = false;
    let mut path = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--json" => json = true,
            "--dump-text" => dump = true,
            "--style" => style = true,
            "--sign-list" => sign_list = true,
            _ if a.starts_with('-') && a != "-" => return fail(json, &format!("모르는 옵션: {a}\n\n{USAGE}")),
            _ if path.is_some() => return fail(json, &format!("파일은 하나만 줄 수 있습니다: {a}")),
            _ => path = Some(a),
        }
    }

    let mut bytes = vec![];
    let read = match path.as_deref() {
        Some(p) if p != "-" => std::fs::read(p).map(|b| bytes = b),
        _ => std::io::stdin().read_to_end(&mut bytes).map(|_| ()),
    };
    let text = match read.map_err(|e| format!("읽기 실패: {e}")).and_then(|_| input::to_text(&bytes)) {
        Ok(t) => t,
        Err(e) => return fail(json, &e),
    };
    if dump {
        print!("{text}");
        return ExitCode::SUCCESS;
    }
    if sign_list {
        let (block, warnings) = checks::sign_block(&text);
        if json {
            let w: Vec<String> = warnings.iter().map(|s| json_str(s)).collect();
            println!(r#"{{"block":{},"warnings":[{}]}}"#, json_str(&block), w.join(","));
        } else {
            print!("{block}");
            warnings.iter().for_each(|w| eprintln!("WARN {w}"));
        }
        return ExitCode::SUCCESS;
    }

    let spec = spec::parse(&text);
    let mut reports = vec![
        checks::sanggi(&spec),
        checks::numerals(&text),
        checks::repeats(&text),
        checks::josa(&text),
        checks::figures(&spec),
        checks::citations(&spec),
        checks::vague(&spec),
        checks::signs(&spec, &text),
        checks::claim_form(&spec),
        checks::support(&spec),
    ];
    if style {
        reports.push(checks::style(&text));
    }
    let total: usize = reports.iter().map(|r| r.problems.len()).sum();

    if json {
        println!("{}", to_json(&reports, total));
    } else {
        for r in &reports {
            let status = if r.problems.is_empty() { "문제 없음".to_string() } else { format!("{}건", r.problems.len()) };
            println!("■ {} — {status}", r.title);
            r.info.iter().for_each(|l| println!("    {l}"));
            r.problems.iter().for_each(|p| println!("  ✗ {p}"));
            println!();
        }
        println!("합계 {total}건 (참고용 — 최종 판단은 사람이)");
    }
    if total > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// 종료 코드 2. --json이면 오류도 JSON으로 표준 출력에 낸다.
fn fail(json: bool, msg: &str) -> ExitCode {
    if json {
        println!(r#"{{"error":{}}}"#, json_str(msg));
    } else {
        eprintln!("{msg}");
    }
    ExitCode::from(2)
}

fn to_json(reports: &[checks::Report], total: usize) -> String {
    let opt = |v: Option<String>| v.unwrap_or("null".into());
    let items: Vec<String> = reports
        .iter()
        .map(|r| {
            let info: Vec<String> = r.info.iter().map(|s| json_str(s)).collect();
            let problems: Vec<String> = r
                .problems
                .iter()
                .map(|p| {
                    format!(
                        r#"{{"message":{},"claim":{},"line":{},"find":{},"replace":{}}}"#,
                        json_str(&p.msg),
                        opt(p.claim.map(|c| c.to_string())),
                        opt(p.line.map(|l| l.to_string())),
                        opt(p.fix.as_ref().map(|f| json_str(&f.0))),
                        opt(p.fix.as_ref().map(|f| json_str(&f.1))),
                    )
                })
                .collect();
            format!(
                r#"{{"id":{},"title":{},"info":[{}],"problems":[{}]}}"#,
                json_str(r.id),
                json_str(r.title),
                info.join(","),
                problems.join(",")
            )
        })
        .collect();
    format!(r#"{{"total":{total},"reports":[{}]}}"#, items.join(","))
}

fn json_str(s: &str) -> String {
    let mut o = String::from('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_escapes() {
        assert_eq!(super::json_str("\"상기\"\\\n\t"), r#""\"상기\"\\\n\u0009""#);
    }
}
