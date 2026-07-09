# Pins: CPython 3.12 keyword-accepting str methods.
# split/rsplit/expandtabs/encode take kwargs; replace/center/etc. are
# positional-only (covered by unexpected-kw TypeError elsewhere).
print("a,b,c".split(sep=",", maxsplit=1))
print("a b c".split(maxsplit=1))
print("a-b-c".rsplit(sep="-", maxsplit=1))
print("a b c".rsplit(maxsplit=1))
print("a\tb".expandtabs(tabsize=4))
print("abc".encode(encoding="utf-8"))
