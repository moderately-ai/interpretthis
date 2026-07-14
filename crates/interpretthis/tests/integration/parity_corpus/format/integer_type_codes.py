# format() integer type codes handle big ints, treat bool as int under a code,
# raise on unknown/incompatible codes, and raise OverflowError for :c out of
# range. Regression: a stale catch-all rendered BigInt as decimal, printed bool
# as "True"/"False" under :d, and silently ignored bad codes / out-of-range :c.
print(f"{2**100:x}")
print(f"{2**80:#X}")
print(f"{2**70:b}")
print(f"{-255:x}")            # negative radix keeps the sign
print(f"{-5:b}")
print(f"{10**30:.2f}")        # big int under a float code

# bool under a numeric code is 0/1; bare bool still prints True/False.
print(f"{True:d}", f"{False:x}", f"{True}")

# :c renders a code point; out of range / negative / too-big raise OverflowError.
print(f"{65:c}")
for bad in (0x110000, -1, 2 ** 100):
    try:
        format(bad, "c")
    except OverflowError:
        print("OverflowError")

# Unknown / incompatible codes raise ValueError.
try:
    format(5, "q")
except ValueError:
    print("ValueError")
try:
    format(1.5, "d")
except ValueError:
    print("ValueError")
