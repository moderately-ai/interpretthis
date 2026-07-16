# repr escapes every non-printable char (str.isprintable() == False) exactly as
# CPython: \xNN up to U+00FF, \uNNNN to U+FFFF, then \UNNNNNNNN. Printable
# non-ASCII (letters, emoji) stays verbatim. isprintable() shares the predicate.
# Invisible chars are built via \u escapes so this source stays ASCII.
print(repr(chr(0x80)), repr(chr(0x9f)), repr(chr(0xa0)))
print(repr("­"))  # soft hyphen (Cf)
print(repr("​"))  # zero-width space (Cf)
print(repr(" "), repr(" "))  # line / paragraph separator (Zl / Zp)
print(repr("﻿"))  # BOM / zero-width no-break space (Cf)
print(repr("normal text"))
print(repr("café"))  # printable non-ASCII stays verbatim
print(repr("日本語"))
print(repr("\U0001F600"))  # emoji, printable
print(repr("\U000e0001"))  # tag char (Cf), escaped
print(repr(chr(0x7f)), repr(chr(0x1f)), repr(chr(0)))
print("\x80".isprintable(), "\xa0".isprintable(), "abc".isprintable())
print("​".isprintable(), "café".isprintable(), " ".isprintable())
print("".isprintable(), "a b".isprintable(), "\n".isprintable())
print(" ".isprintable(), "﻿".isprintable())

# Escaping is identical when the string is nested inside a container repr.
print(["\x80", "\xa0", "ok"])
print({"k": "​"})
print(("\x00", "\x7f"))
