from decimal import Decimal as D

# as_tuple() -> DecimalTuple(sign, digits, exponent), trailing zeros preserved.
for s in ["0.1", "1.00", "123.45", "0", "100", "-123", "1E3", "0.001", "-5.5"]:
    print(repr(s), D(s).as_tuple())

# sqrt of a perfect square uses the ideal exponent (no padding to 28 digits);
# an inexact root keeps the context's 28 significant digits.
for s in ["9", "4", "2", "9.00", "4.00", "100", "0", "1", "16",
          "2.25", "6.25", "0.25", "1.44", "9.0", "9.000"]:
    print(repr(s), "->", D(s).sqrt())
