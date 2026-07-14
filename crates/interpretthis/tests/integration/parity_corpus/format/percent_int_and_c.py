# printf-style % formatting handles big ints across d/o/x/X and treats %c as a
# codepoint-or-single-char conversion. Regression: %d/%o/%x errored on a
# promoted BigInt, %c rejected strings and floats inconsistently, and %c raised
# ValueError instead of OverflowError out of range.
print("%d" % (2**100))
print("%o" % (2**70))
print("%x" % (2**80))
print("%X" % (2**80))

print("%c" % 65)              # 'A'
print("%c" % True)            # '\x01' codepoint
print("%c" % "z")             # single-char string passes through
print("%5c" % 65)             # width padding still applies

# %c out of range / negative / big raise OverflowError.
for bad in (0x110000, -1, 2 ** 100):
    try:
        "%c" % bad
    except OverflowError:
        print("OverflowError")

# %c rejects a float and a multi-char string with TypeError.
try:
    "%c" % 3.9
except TypeError:
    print("TypeError")
try:
    "%c" % "ab"
except TypeError:
    print("TypeError")
