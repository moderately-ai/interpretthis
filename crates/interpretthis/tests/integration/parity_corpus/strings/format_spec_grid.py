# Pins: builtin format-spec mini-language covers fixed-point,
# padding/alignment, zero-fill, hex/binary, thousands-separator.
print(f"{3.14159:.2f}")
print(f"{42:>10}")
print(f"{42:<10}|")
print(f"{42:08d}")
print(f"{255:x}")
print(f"{255:#x}")
print(f"{1234567:,}")
