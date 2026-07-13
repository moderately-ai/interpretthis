# base64.b64encode / b64decode round-trip pins the Engine::encode/decode
# wiring against python3's standard alphabet.
import base64
encoded = base64.b64encode(b"hello world")
print(encoded)
decoded = base64.b64decode(encoded)
print(decoded)
# Empty input
print(base64.b64encode(b""))
print(base64.b64decode(b""))
# Padding edge: "Man" -> "TWFu" (no padding), "Ma" -> "TWE=", "M" -> "TQ=="
print(base64.b64encode(b"Man"))
print(base64.b64encode(b"Ma"))
print(base64.b64encode(b"M"))
