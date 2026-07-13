# Pins: `and`/`or` return the operand value, not a coerced bool.
# 0 is falsy so `0 or "default"` evaluates to "default"; "value" is truthy
# so `"value" and "other"` evaluates to "other".
a = 0 or "default"
b = "value" and "other"
c = None or 42
d = "" or "fallback"
print(f"{a},{b},{c},{d}")
