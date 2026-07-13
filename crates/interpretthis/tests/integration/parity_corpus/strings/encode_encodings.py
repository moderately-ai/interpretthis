# Pins: encode utf-8 / ascii / latin-1.
print("hi".encode("utf-8"))
print("hi".encode("ascii"))
print("café".encode("latin-1"))
try:
    "café".encode("ascii")
except Exception as e:
    print(type(e).__name__)
