// Fingerprints of the 2.34.0 native sheet, excluding the relying-party name and
// variable QR geometry. CDP also lists unrelated data: pages. Never log this URL.
use crate::asar::sha256_hex;

const SHEET_HASH: &str = "789aa0b4c90a4b4b291448ee6e92d780d050e1785ccf6c4d988d1ecc62c7ef4d";
const SVG_HASH: &str = "974b263396163f1c7512763956c2475e1680e7be935502d779f21d33e3e733b2";
const BEFORE_RP: &str = "device%20that%20has%20your%20passkey%20for%20";
const BEFORE_SVG: &str = "%3Cdiv%20class%3D%22qr-centered-view%22%3E";
const END_SVG: &str = "%3C%2Fsvg%3E";
const CIRCLE: &str = "%3Ccircle%20cx%3D%22~%22%20cy%3D%22~%22%20r%3D%22~%22%2F%3E";

pub fn is_native_sheet(url: &str) -> bool {
    let Some(html) = url.strip_prefix("data:text/html;charset=utf-8,") else {
        return false;
    };
    if !html.starts_with("%3C!DOCTYPE%20html%3E") || html.len() > 1024 * 1024 || !html.is_ascii() {
        return false;
    }
    let Some((head, rest)) = html.split_once(BEFORE_RP) else {
        return false;
    };
    let Some(rp_end) = rest.find('%') else {
        return false;
    };
    let (rp, rest) = rest.split_at(rp_end);
    if rp.is_empty()
        || !rp
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
    {
        return false;
    }
    let Some((middle, rest)) = rest.split_once(BEFORE_SVG) else {
        return false;
    };
    let Some((svg, tail)) = rest.split_once(END_SVG) else {
        return false;
    };
    let outer = [head, BEFORE_RP, middle, BEFORE_SVG, tail].concat();
    if sha256_hex(outer.as_bytes()) != SHEET_HASH {
        return false;
    }

    // Preserve every escaped byte and tag/attribute name. Only geometry and
    // repeated drawing primitives vary between the official encoder's outputs.
    let encoded = [svg, END_SVG].concat();
    let bytes = encoded.as_bytes();
    let mut shape = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                shape.push_str(&encoded[i..i + 3]);
                i += 3;
            }
            b'0'..=b'9' | b'.' => {
                shape.push('~');
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
            }
            b => {
                shape.push(char::from(b));
                i += 1;
            }
        }
    }
    sha256_hex(shape.replace(CIRCLE, "").as_bytes()) == SVG_HASH
}
