# int(str, base): validate the base (no panic), auto-detect prefixes, accept
# underscores, and parse to arbitrary precision. Regression: an unvalidated base
# was passed to Rust's from_str_radix, which PANICS on base 0 or > 36; a base with
# a non-string argument was silently ignored; underscores were rejected; and a
# long literal overflowed i64.

# base 0 auto-detects the prefix.
print(int("0x1f", 0))
print(int("0o17", 0))
print(int("0b101", 0))
print(int("42", 0))
print(int("-0xff", 0))

# explicit bases, with and without the matching prefix.
print(int("ff", 16))
print(int("0xFF", 16))
print(int("777", 8))
print(int("101", 2))
print(int("z", 36))

# underscores between digits.
print(int("1_000_000"))
print(int("0xff_ff", 16))

# arbitrary precision — exact, not truncated to i64.
print(int("9" * 40))

# error cases (base out of range / bad literal / base with non-string).
for label, thunk in [
    ("base-0-panic", lambda: int("10", 0)),   # valid in py: -> 10
    ("base-40", lambda: int("10", 40)),
    ("base-1", lambda: int("10", 1)),
    ("bad-literal", lambda: int("12", 2)),
    ("non-string-base", lambda: int(255, 16)),
    ("double-underscore", lambda: int("1__0")),
]:
    try:
        print(label, "=", thunk())
    except ValueError:
        print(label, "ValueError")
    except TypeError:
        print(label, "TypeError")

# int() of an already-big int returns it.
print(int(2**70))
