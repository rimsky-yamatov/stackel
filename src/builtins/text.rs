//! Pure string words. Positions and lengths count characters (Unicode code points).

use crate::error::Result;
use crate::eval::Interp;
use crate::value::Value;

pub const WORDS: &[&str] = &[
    "substring",
    "index_of",
    "ord",
    "chr",
    "trim",
    "upper",
    "lower",
    "parse_int",
    "parse_float",
];

pub fn call(vm: &mut Interp, name: &str) -> Result<bool> {
    match name {
        // String Int Int -- String
        // `s start end substring` is the characters from `start` up to `end`. Both positions are
        // moved into the string, and an `end` before `start` gives "".
        "substring" => {
            let end = vm.pop_int()?;
            let start = vm.pop_int()?;
            let s = vm.pop_str()?;
            let len = s.chars().count() as i64;
            let start = start.clamp(0, len);
            let end = end.clamp(start, len);
            let piece: String = s
                .chars()
                .skip(start as usize)
                .take((end - start) as usize)
                .collect();
            vm.push(Value::Str(piece));
        }
        // String String -- Int
        // `hay needle index_of` is where the first `needle` starts, or -1.
        "index_of" => {
            let needle = vm.pop_str()?;
            let hay = vm.pop_str()?;
            let at = match hay.find(&needle) {
                Some(byte) => hay[..byte].chars().count() as i64,
                None => -1,
            };
            vm.push(Value::Int(at));
        }
        // String -- Int   (the code point of the first character, or -1 for "")
        "ord" => {
            let s = vm.pop_str()?;
            let code = s.chars().next().map_or(-1, |c| c as u32 as i64);
            vm.push(Value::Int(code));
        }
        // Int -- String   (the character with this code point, or "" if there is none)
        "chr" => {
            let n = vm.pop_int()?;
            let s = if (0..=0x10FFFF).contains(&n) {
                char::from_u32(n as u32)
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            vm.push(Value::Str(s));
        }
        // String -- String   (removes white space at both ends)
        "trim" => {
            let s = vm.pop_str()?;
            vm.push(Value::Str(s.trim().to_string()));
        }
        "upper" => {
            let s = vm.pop_str()?;
            vm.push(Value::Str(s.to_uppercase()));
        }
        "lower" => {
            let s = vm.pop_str()?;
            vm.push(Value::Str(s.to_lowercase()));
        }
        // String -- Int Bool   (the number, and whether the string was a number; 0 if it was not)
        "parse_int" => {
            let s = vm.pop_str()?;
            match s.parse::<i64>() {
                Ok(n) => {
                    vm.push(Value::Int(n));
                    vm.push(Value::Bool(true));
                }
                Err(_) => {
                    vm.push(Value::Int(0));
                    vm.push(Value::Bool(false));
                }
            }
        }
        // String -- Float Bool
        "parse_float" => {
            let s = vm.pop_str()?;
            match s.parse::<f64>() {
                Ok(x) if x.is_finite() => {
                    vm.push(Value::Float(x));
                    vm.push(Value::Bool(true));
                }
                _ => {
                    vm.push(Value::Float(0.0));
                    vm.push(Value::Bool(false));
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
