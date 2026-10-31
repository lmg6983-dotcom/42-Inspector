mod checks;
mod spec;

use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "기재불비 검사기\n\n사용법: inspector42 [명세서.txt]\n  파일을 주지 않으면 표준 입력에서 읽는다 (UTF-8).\n  지적 사항이 있으면 종료 코드 1.";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    if arg.as_deref().is_some_and(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let mut bytes = vec![];
    let read = match &arg {
        Some(path) => std::fs::read(path).map(|b| bytes = b),
        None => std::io::stdin().read_to_end(&mut bytes).map(|_| ()),
    };
    if let Err(e) = read {
        eprintln!("읽기 실패: {e}");
        return ExitCode::from(2);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        eprintln!("UTF-8 텍스트가 아닙니다. 메모장에서 'UTF-8'로 다시 저장해 주세요.");
        return ExitCode::from(2);
    };
    let text = text.trim_start_matches('\u{feff}');

    let spec = spec::parse(text);
    let reports = [
        checks::sanggi(&spec),
        checks::numerals(text),
        checks::repeats(text),
        checks::josa(text),
        checks::figures(&spec),
        checks::citations(&spec),
    ];

    let mut total = 0;
    for r in &reports {
        println!("■ {} — {}", r.title, if r.problems.is_empty() { "문제 없음".to_string() } else { format!("{}건", r.problems.len()) });
        for line in &r.info {
            println!("    {line}");
        }
        for p in &r.problems {
            println!("  ✗ {p}");
        }
        println!();
        total += r.problems.len();
    }
    println!("합계 {total}건 (참고용 — 최종 판단은 사람이)");
    if total > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
