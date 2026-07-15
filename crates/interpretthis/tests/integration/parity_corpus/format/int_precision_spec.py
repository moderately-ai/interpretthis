# Precision is not allowed with integer presentation types (or no type).
for spec in [".3", ".0", ".5"]:
    try:
        format(100, spec)
        print("no error", spec)
    except ValueError as e:
        print("VE", str(e))
try:
    f"{100:.3}"
except ValueError as e:
    print("VE-fstring", str(e))
# The type character must be the last of the spec; trailing chars are invalid.
for spec in ["d.2", "b.2", "x.3", "X.1", "o.2", "c.2"]:
    try:
        format(65, spec)
        print("no error", spec)
    except ValueError as e:
        print("VE-order", str(e))
# Precision IS allowed for the float presentation codes, even on an int.
print(format(100, ".2f"), format(100, ".3e"), format(100, ".3g"), format(100, ".1%"))
# A float with a bare precision is unaffected.
print(format(3.14159, ".3"), f"{3.14159:.2}")
