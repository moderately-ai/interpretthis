# json.loads preserves integers beyond i64 as exact ints, not lossy floats.
# Regression: a big integer went through as_f64 and came back as 1.23e+29.
import json

print(json.loads("123456789012345678901234567890"))
print(json.loads("-98765432109876543210"))
print(json.loads("123") + 1)                 # small int unchanged
print(json.loads("1.5"))                     # float stays float
print(json.loads("1e3"))                     # exponent -> float
print(json.loads("[1, 99999999999999999999999, 2]"))
print(json.loads('{"big": 100000000000000000000}')["big"] * 2)
