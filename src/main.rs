mod checks;
mod input;
mod spec;

use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "기재불비 검사기

사용법: inspector42 [--json] [명세서 파일]
  파일: TXT(UTF-8 · UTF-16 · CP949), DOCX, HWPX, HWP 5.0
  파일을 주지 않으면 표준 입력에서 읽는다.
  --json  결과를 JSON으로 출력
  지적 사항이 있으면 종료 코드 1, 읽기 실패는 2.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let json = args.iter().any(|a| a == "--json");
    let path = args.iter().find(|a| !a.starts_with("--"));

    let mut bytes = vec![];
    let read = match path {
        Some(p) => std::fs::read(p).map(|b| bytes = b),
        None => std::io::stdin().read_to_end(&mut bytes).map(|_| ()),
    };
    let text = match read.map_err(|e| format!("읽기 실패: {e}")).and_then(|_| input::to_text(&bytes)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let spec = spec::parse(&text);
    let reports = [
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

fn to_json(reports: &[checks::Report], total: usize) -> String {
    let list = |v: &[String]| format!("[{}]", v.iter().map(|s| json_str(s)).collect::<Vec<_>>().join(","));
    let items: Vec<String> = reports
        .iter()
        .map(|r| format!(r#"{{"check":{},"info":{},"problems":{}}}"#, json_str(r.title), list(&r.info), list(&r.problems)))
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
