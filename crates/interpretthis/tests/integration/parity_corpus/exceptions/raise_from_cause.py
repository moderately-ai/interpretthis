# Pins: `raise X from Y` syntax; the named effect (ValueError) is caught.
try:
    raise ValueError("effect") from TypeError("cause")
except ValueError as e:
    print("caught")
