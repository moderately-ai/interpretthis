print(repr(chr(0x80)), repr(chr(0x9f)), repr(chr(0xa0)))
print(repr(chr(0xad)))  # soft hyphen (Cf)
print(repr("​"))   # zero-width space (Cf)
print(repr(" "), repr(" "))  # line/para separator (Zl/Zp)
print(repr("﻿"))   # BOM (Cf)
print(repr("normal text"))
print(repr("café"))     # printable non-ASCII stays verbatim
print(repr("日本語"))
print(repr("\U0001F600"))  # emoji, printable
print(repr("\U000e0001"))  # tag char (Cf), escaped
print("\x80".isprintable(), "\xa0".isprintable(), "abc".isprintable())
print("​".isprintable(), "café".isprintable(), " ".isprintable())
print("".isprintable(), "a b".isprintable(), "\n".isprintable())
print(["\x80", "\xa0"])
print({"k": "\x9f"})
print(repr(chr(0x7f)), repr(chr(0x1f)))
