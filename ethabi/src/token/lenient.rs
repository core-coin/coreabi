// Copyright 2015-2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
	errors::Error,
	token::{StrictTokenizer, Tokenizer},
	Uint,
};
use std::borrow::Cow;

use once_cell::sync::Lazy;
static RE: Lazy<regex::Regex> =
	Lazy::new(|| regex::Regex::new(r"^([0-9]+)(\.[0-9]+)?\s*(core|nucle|ore)$").expect("invalid regex"));

/// Tries to parse string as a token. Does not require string to clearly represent the value.
pub struct LenientTokenizer;

impl Tokenizer for LenientTokenizer {
	fn tokenize_address(value: &str) -> Result<[u8; 22], Error> {
		StrictTokenizer::tokenize_address(value)
	}

	fn tokenize_string(value: &str) -> Result<String, Error> {
		StrictTokenizer::tokenize_string(value)
	}

	fn tokenize_bool(value: &str) -> Result<bool, Error> {
		StrictTokenizer::tokenize_bool(value)
	}

	fn tokenize_bytes(value: &str) -> Result<Vec<u8>, Error> {
		StrictTokenizer::tokenize_bytes(value)
	}

	fn tokenize_fixed_bytes(value: &str, len: usize) -> Result<Vec<u8>, Error> {
		StrictTokenizer::tokenize_fixed_bytes(value, len)
	}

	fn tokenize_uint(value: &str) -> Result<[u8; 32], Error> {
		let result = StrictTokenizer::tokenize_uint(value);
		if result.is_ok() {
			return result;
		}

		// Tries to parse it as is first. If it fails, tries to check for
		// expectable units with the following format: 'Number[Spaces]Unit'.
		//   If regex fails, then the original FromDecStrErr should take priority
		let uint = match Uint::from_dec_str(value) {
			Ok(_uint) => _uint,
			Err(dec_error) => {
				let original_dec_error = dec_error.to_string();

				match RE.captures(value) {
					Some(captures) => {
						let integer = captures.get(1).expect("capture group does not exist").as_str();
						let fract = captures.get(2).map(|c| c.as_str().trim_start_matches('.')).unwrap_or_else(|| "");
						let units = captures.get(3).expect("capture group does not exist").as_str();

						let units = Uint::from(match units.to_lowercase().as_str() {
							"core" => 18,
							"nucle" => 9,
							"ore" => 0,
							_ => return Err(Error::InvalidData),
						});

						let integer =
							Uint::from_dec_str(integer).expect("FAILED").checked_mul(Uint::from(10u32).pow(units));

						if fract.is_empty() {
							integer.ok_or(dec_error).expect("FAILED")
						} else {
							// makes sure we don't go beyond 18 decimals
							let fract_pow =
								units.checked_sub(Uint::from(fract.len())).ok_or(dec_error).expect("FAILED");

							let fract = Uint::from_dec_str(fract)
								.expect("FAILED")
								.checked_mul(Uint::from(10u32).pow(fract_pow))
								.ok_or_else(|| Error::Other(Cow::Owned(original_dec_error.clone())))?;

							integer
								.and_then(|integer| integer.checked_add(fract))
								.ok_or(Error::Other(Cow::Owned(original_dec_error)))?
						}
					}
					None => return Err(Error::InvalidData),
				}
			}
		};

		Ok(uint.into())
	}

	// We don't have a proper signed int 256-bit long type, so here we're cheating. We build a U256
	// out of it and check that it's within the lower/upper bound of a hypothetical I256 type: half
	// the `U256::max_value().
	fn tokenize_int(value: &str) -> Result<[u8; 32], Error> {
		let result = StrictTokenizer::tokenize_int(value);
		if result.is_ok() {
			return result;
		}

		let abs = Uint::from_dec_str(value.trim_start_matches('-')).expect("FAILED");
		let max = Uint::max_value() / 2;
		let int = if value.starts_with('-') {
			if abs.is_zero() {
				return Ok(abs.into());
			} else if abs > max + 1 {
				return Err(Error::Other(Cow::Borrowed("int256 parse error: Underflow")));
			}
			!abs + 1 // two's complement
		} else {
			if abs > max {
				return Err(Error::Other(Cow::Borrowed("int256 parse error: Overflow")));
			}
			abs
		};
		Ok(int.into())
	}
}

#[cfg(test)]
mod tests {

	use crate::{
		token::{LenientTokenizer, Token, Tokenizer},
		ParamType, Uint,
	};

	#[test]
	fn tokenize_uint() {
		assert_eq!(
			LenientTokenizer::tokenize(
				&ParamType::Uint(256),
				"1111111111111111111111111111111111111111111111111111111111111111"
			)
			.unwrap(),
			Token::Uint([0x11u8; 32].into())
		);
	}

	#[test]
	fn tokenize_uint_wei() {
		assert_eq!(LenientTokenizer::tokenize(&ParamType::Uint(256), "1ore").unwrap(), Token::Uint(Uint::from(1)));

		assert_eq!(LenientTokenizer::tokenize(&ParamType::Uint(256), "1 ore").unwrap(), Token::Uint(Uint::from(1)));
	}

	#[test]
	fn tokenize_uint_gwei() {
		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "1nucle").unwrap(),
			Token::Uint(Uint::from_dec_str("1000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "1nucle").unwrap(),
			Token::Uint(Uint::from_dec_str("1000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "1nucle").unwrap(),
			Token::Uint(Uint::from_dec_str("1000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "0.1 nucle").unwrap(),
			Token::Uint(Uint::from_dec_str("100000000").unwrap())
		);
	}

	#[test]
	fn tokenize_uint_core() {
		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "10000000000core").unwrap(),
			Token::Uint(Uint::from_dec_str("10000000000000000000000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "1core").unwrap(),
			Token::Uint(Uint::from_dec_str("1000000000000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "0.01 core").unwrap(),
			Token::Uint(Uint::from_dec_str("10000000000000000").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "0.000000000000000001core").unwrap(),
			Token::Uint(Uint::from_dec_str("1").unwrap())
		);

		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Uint(256), "0.000000000000000001core").unwrap(),
			LenientTokenizer::tokenize(&ParamType::Uint(256), "1ore").unwrap(),
		);
	}

	#[test]
	fn tokenize_uint_array_core() {
		assert_eq!(
			LenientTokenizer::tokenize(&ParamType::Array(Box::new(ParamType::Uint(256))), "[1core,0.1 core]").unwrap(),
			Token::Array(vec![
				Token::Uint(Uint::from_dec_str("1000000000000000000").unwrap()),
				Token::Uint(Uint::from_dec_str("100000000000000000").unwrap())
			])
		);
	}
}
