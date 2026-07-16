# `%a` is ascii(): repr with every non-ASCII code point backslash-escaped.
print("%a" % "café")
print("%a %a" % ("naïve", "日本"))
print("%r %s %a" % ("café", "café", "café"))
print("%a" % 42)
print("%a" % [1, "x"])
print("%-10a|" % "é")
print("%a" % "\x00\x7f\x80")


class C:
    def __repr__(self):
        return "Cùstom"


print("%a" % C())
