//! 파일 → 텍스트. 형식은 확장자가 아니라 파일 앞부분으로 판별한다.
//! TXT(UTF-8 · UTF-16 · CP949), DOCX, HWPX, HWP 5.0

use regex::Regex;
use std::io::{Cursor, Read};
use std::sync::LazyLock;

pub fn to_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(b"PK\x03\x04") {
        from_zip(bytes)
    } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        from_hwp(bytes)
    } else {
        decode(bytes)
    }
}

fn decode(bytes: &[u8]) -> Result<String, String> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.trim_start_matches('\u{feff}').to_string());
    }
    // BOM이 있으면 UTF-16, 없으면 CP949 (encoding_rs의 EUC-KR은 windows-949 전체)
    let (s, _, bad) = encoding_rs::EUC_KR.decode(bytes);
    if bad {
        return Err("텍스트 인코딩을 알 수 없습니다 (UTF-8 · UTF-16 · CP949 지원)".into());
    }
    Ok(s.into_owned())
}

// ───────────── DOCX · HWPX ─────────────

fn from_zip(bytes: &[u8]) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("ZIP 열기 실패: {e}"))?;
    let mut parts: Vec<(u32, String)> = zip
        .file_names()
        .filter_map(|n| {
            let n = n.ok()?;
            Some((n.strip_prefix("Contents/section")?.strip_suffix(".xml")?.parse().ok()?, n.to_string()))
        })
        .collect();
    parts.sort();
    let mut names: Vec<String> = parts.into_iter().map(|(_, n)| n).collect();
    if names.is_empty() && zip.by_name("word/document.xml").is_ok() {
        names.push("word/document.xml".into());
    }
    if names.is_empty() {
        return Err("DOCX · HWPX 문서가 아닌 ZIP 파일입니다".into());
    }
    let mut out = String::new();
    for name in names {
        let mut xml = String::new();
        zip.by_name(&name)
            .map_err(|e| e.to_string())
            .and_then(|mut f| f.read_to_string(&mut xml).map_err(|e| e.to_string()))
            .map_err(|e| format!("{name} 읽기 실패: {e}"))?;
        out.push_str(&xml_text(&xml));
    }
    Ok(out)
}

static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(/?)(?:[\w.-]+:)?([\w.-]+)[^>]*?(/?)>|([^<]+)").unwrap());
static ENTITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&(#x[0-9A-Fa-f]+|#\d+|\w+);").unwrap());

/// DOCX(w:p · w:t)와 HWPX(hp:p · hp:t)는 접두사만 다르고 구조가 같다:
/// 문단 <p>, 글자 <t>, 탭 <tab/>, 줄바꿈 <br/> · <lineBreak/>
fn xml_text(xml: &str) -> String {
    let mut out = String::new();
    let mut in_t = 0u32;
    for c in TAG.captures_iter(xml) {
        if let Some(text) = c.get(4) {
            if in_t > 0 {
                out.push_str(&unescape(text.as_str()));
            }
            continue;
        }
        let (close, empty) = (&c[1] == "/", &c[3] == "/");
        match &c[2] {
            "t" if !close && !empty => in_t += 1,
            "t" if close => in_t = in_t.saturating_sub(1),
            "p" if close || empty => out.push('\n'),
            "tab" if !close => out.push('\t'),
            "br" | "lineBreak" if !close => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn unescape(s: &str) -> String {
    ENTITY
        .replace_all(s, |c: &regex::Captures| {
            let e = &c[1];
            let code = match e {
                "lt" => Some('<' as u32),
                "gt" => Some('>' as u32),
                "amp" => Some('&' as u32),
                "quot" => Some('"' as u32),
                "apos" => Some('\'' as u32),
                _ if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok(),
                _ if e.starts_with('#') => e[1..].parse().ok(),
                _ => None,
            };
            code.and_then(char::from_u32).map_or(c[0].to_string(), String::from)
        })
        .into_owned()
}

// ───────────── HWP 5.0 ─────────────

const PARA_HEADER: u32 = 66;
const PARA_TEXT: u32 = 67;

fn from_hwp(bytes: &[u8]) -> Result<String, String> {
    let mut cf = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(|e| format!("HWP 열기 실패: {e}"))?;
    let mut header = vec![];
    cf.open_stream("/FileHeader")
        .and_then(|mut s| s.read_to_end(&mut header))
        .map_err(|_| "HWP 5.0 문서가 아닙니다 (DOC 같은 다른 OLE 파일은 지원 안 함)")?;
    if !header.starts_with(b"HWP Document File") || header.len() < 40 {
        return Err("HWP 5.0 문서가 아닙니다".into());
    }
    let props = u32::from_le_bytes(header[36..40].try_into().unwrap());
    if props & 0b10 != 0 {
        return Err("암호가 걸린 HWP는 읽을 수 없습니다. 암호를 풀고 다시 저장해 주세요".into());
    }
    if props & 0b100 != 0 {
        return Err("배포용 HWP는 읽을 수 없습니다. 일반 문서로 저장해 주세요".into());
    }
    let mut out = String::new();
    for i in 0.. {
        let Ok(mut stream) = cf.open_stream(format!("/BodyText/Section{i}")) else { break };
        let mut raw = vec![];
        stream.read_to_end(&mut raw).map_err(|e| format!("Section{i} 읽기 실패: {e}"))?;
        if props & 1 != 0 {
            let mut data = vec![];
            flate2::read::DeflateDecoder::new(&raw[..])
                .read_to_end(&mut data)
                .map_err(|e| format!("Section{i} 압축 풀기 실패: {e}"))?;
            raw = data;
        }
        records(&raw, &mut out);
    }
    Ok(out)
}

/// 레코드 머리: 태그 10비트 · 레벨 10비트 · 크기 12비트 (0xFFF면 다음 4바이트가 크기)
fn records(data: &[u8], out: &mut String) {
    let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut i = 0;
    while let Some(h) = u32_at(i) {
        i += 4;
        let mut size = (h >> 20) as usize;
        if size == 0xFFF {
            let Some(s) = u32_at(i) else { break };
            size = s as usize;
            i += 4;
        }
        let Some(body) = data.get(i..i + size) else { break };
        i += size;
        match h & 0x3FF {
            PARA_HEADER if !out.is_empty() && !out.ends_with('\n') => out.push('\n'),
            PARA_TEXT => para_text(body, out),
            _ => {}
        }
    }
}

/// UTF-16LE. 0~31은 제어 문자: 0 · 10 · 13 · 24~31은 1칸, 나머지는 8칸(뒤 7칸은 부가 정보)
fn para_text(body: &[u8], out: &mut String) {
    let units: Vec<u16> = body.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let mut text = vec![];
    let mut k = 0;
    while k < units.len() {
        let u = units[k];
        let (ch, width) = match u {
            9 => (Some('\t' as u16), 8),
            10 | 13 => (Some('\n' as u16), 1),
            24 => (Some('-' as u16), 1),
            30 | 31 => (Some(' ' as u16), 1),
            0 | 25..=29 => (None, 1),
            1..=31 => (None, 8),
            _ => (Some(u), 1),
        };
        text.extend(ch);
        k += width;
    }
    out.push_str(&String::from_utf16_lossy(&text));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(vec![]));
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn text_encodings() {
        let (cp949, _, _) = encoding_rs::EUC_KR.encode("【청구항 1】 기판(10)");
        assert_eq!(to_text(&cp949).unwrap(), "【청구항 1】 기판(10)");
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("상기 전극".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(to_text(&utf16).unwrap(), "상기 전극");
        assert_eq!(to_text("\u{feff}가".as_bytes()).unwrap(), "가");
    }

    #[test]
    fn docx_and_hwpx() {
        let docx = zip_of(&[(
            "word/document.xml",
            r#"<?xml version="1.0"?><w:document><w:body><w:p><w:pPr><w:tabs><w:tab w:val="left"/></w:tabs></w:pPr><w:r><w:t>【청구항 1】</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">기판(10) </w:t></w:r><w:r><w:t>&amp; 전극</w:t></w:r></w:p><w:p/></w:body></w:document>"#,
        )]);
        assert_eq!(to_text(&docx).unwrap().trim(), "【청구항 1】\n기판(10) & 전극");

        let p = |t: &str| format!(r#"<hp:p id="0"><hp:run charPrIDRef="0"><hp:t>{t}</hp:t></hp:run></hp:p>"#);
        let hwpx = zip_of(&[
            ("mimetype", "application/hwp+zip"),
            ("Contents/section1.xml", &format!("<hs:sec>{}</hs:sec>", p("둘째"))),
            ("Contents/section0.xml", &format!("<hs:sec>{}{}</hs:sec>", p("첫째<hp:tab width=\"4000\"/>칸"), p("&lt;둘&gt;"))),
        ]);
        assert_eq!(to_text(&hwpx).unwrap(), "첫째\t칸\n<둘>\n둘째\n");
    }

    #[test]
    fn hwp_records() {
        let rec = |tag: u32, body: &[u8]| {
            let mut v = (tag | ((body.len() as u32) << 20)).to_le_bytes().to_vec();
            v.extend(body);
            v
        };
        let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
        // 문단 1: "가" + 확장 제어(11, 8칸) + "나" + 문단 끝(13). 문단 2: "다" (끝 문자 없음)
        let mut para1 = utf16("가");
        para1.extend([11u16, 0, 0, 0, 0, 0, 0, 11].iter().flat_map(|u| u.to_le_bytes()));
        para1.extend(utf16("나\r"));
        let data = [rec(PARA_HEADER, &[0; 4]), rec(PARA_TEXT, &para1), rec(PARA_HEADER, &[]), rec(PARA_TEXT, &utf16("다"))].concat();

        let mut cf = cfb::CompoundFile::create(Cursor::new(vec![])).unwrap();
        let mut header = b"HWP Document File".to_vec();
        header.resize(256, 0);
        header[36] = 1; // 압축
        cf.create_stream("/FileHeader").unwrap().write_all(&header).unwrap();
        cf.create_storage("/BodyText").unwrap();
        let mut z = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
        z.write_all(&data).unwrap();
        cf.create_stream("/BodyText/Section0").unwrap().write_all(&z.finish().unwrap()).unwrap();
        cf.flush().unwrap();
        let bytes = cf.into_inner().into_inner();
        assert_eq!(to_text(&bytes).unwrap(), "가나\n다");

        let mut cf = cfb::CompoundFile::create(Cursor::new(vec![])).unwrap();
        header[36] = 0b10; // 암호
        cf.create_stream("/FileHeader").unwrap().write_all(&header).unwrap();
        cf.flush().unwrap();
        assert!(to_text(&cf.into_inner().into_inner()).unwrap_err().contains("암호"));
    }
}
