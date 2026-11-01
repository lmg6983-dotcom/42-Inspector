# 42-Inspector — 에이전트 안내

특허 명세서 기재불비 검사기 (Rust CLI). 사람용 소개는 README.md.

## 절대 규칙

- **실제 명세서는 기밀이다.** 저장소 · 테스트 · 예시 · 커밋 메시지 · PR · 이슈에 내용(문장, 부품명, 파일명)을 넣지 않는다.
  사용자가 준 파일은 그 경로에서 바로 읽고 복사본 · 변환본을 디스크에 만들지 않는다. 확인이 필요하면 파이프로만 본다.
  테스트 문장은 지어낸 예문으로 쓴다.
- 머지와 배포(태그 · 릴리스)는 사용자가 지시할 때만 한다.

## 명령

```bash
cargo test            # 단위 테스트 (examples/sample.txt를 함께 씀)
cargo clippy          # 경고 0 유지
cargo run -- examples/sample.txt
```

## 도구로 쓸 때

```bash
inspector42 --json 명세서.hwp       # 검사 결과 JSON
inspector42 --dump-text 명세서.hwp  # 파일에서 뽑은 텍스트 (line 번호의 기준)
```

종료 코드: `0` 지적 없음, `1` 지적 있음, `2` 사용법 오류 · 읽기 실패. 모르는 옵션이나 파일 두 개는 `2`.

`--json` 출력:

```json
{"total": 3, "reports": [
  {"id": "particles", "title": "괄호 뒤 조사", "info": [],
   "problems": [{"message": "전극(20)는 → 전극(20)은", "claim": null, "line": 10,
                 "find": "전극(20)는", "replace": "전극(20)은"}]}
]}
```

- `id`는 바뀌지 않는 영문 식별자: `antecedent` `numerals` `repeats` `particles` `figures` `citations` `vague` `sign_list` `claim_form` `support`
- `claim`: 청구항 번호 (`0` = 청구항 머리말이 없어 글 전체를 한 덩어리로 본 경우), `line`: `--dump-text` 출력의 줄 번호 (DOCX · HWP · HWPX는 문단 번호)
- `find` / `replace`: 그대로 바꿔도 되는 지적에만 있다 (괄호 뒤 조사, 반복 단어). 나머지는 사람이 판단할 일이라 `null`
- `--json`에서 오류는 표준 출력에 `{"error": "..."}`, 종료 코드 `2`
- 모든 지적은 참고용이다. 고치기 전에 문맥을 확인한다.

## 구조

| 파일 | 하는 일 |
|---|---|
| `src/input.rs` | 파일 → 텍스트. 형식은 앞 바이트로 판별 (HWP 5.0 · HWPX · DOCX · TXT/CP949) |
| `src/spec.rs` | `【…】` 머리말로 청구항 · 도면의 간단한 설명 · 본문 · 부호의 설명 · 첨부 도면을 나누고 인용항을 푼다 |
| `src/checks.rs` | 검사 10종. 각각 `Report { id, title, info, problems: Vec<Finding> }`를 돌려준다 |
| `src/main.rs` | CLI, 텍스트 · JSON 출력 |
| `skill/inspector42/SKILL.md` | 에이전트 스킬 문서. CI가 실행 파일과 묶어 산출물로 올린다. 옵션 · 출력 · `id`를 바꾸면 함께 고친다 |

## 검사를 고치거나 더할 때

1. `checks.rs`에 함수 하나 (`Report::new("영문_id", "한글 제목")`), 지적은 `r.claim(..)` · `r.add(..)` · `r.fix(..)`
2. `main.rs`의 `reports` 배열에 추가
3. `examples/sample.txt`에 걸리는 예를 넣고 `sample_spec` 테스트에 assert 추가 — 기존 assert가 깨지면 안 된다
4. README 표, 이 파일과 `skill/inspector42/SKILL.md`의 `id` 목록 갱신

원칙: 애매하면 지적하지 않는다. 오탐은 도구를 안 쓰게 만든다.
실제 명세서에서 오탐이 나오면, 원인이 된 문형을 **지어낸 예문**으로 테스트에 옮긴 뒤 고친다.
