# A float formatted with a width but no presentation type keeps its natural
# repr (shortest round-trip), not a forced 6-decimal expansion.
print(f"[{3.14:10}]")
print(f"[{3.14:<10}]")
print(f"[{2.5:8}]")
print(f"[{100.0:6}]")
print(f"[{0.1:g}]", f"[{0.1}]")
print(f"{123.456:10}")
print(f"{1.0:5}", f"{1.5:5}")
print("{:8}".format(3.14159))
print(f"{3.14159:.3}", f"{1.0:.3}", f"{123.456:.2}", f"{0.0001234:.2}")
