print(int("101", base=2), int("101", 2))
print(int("ff", base=16), int("ff", 16))
print(int("777", base=8), int("0o777", 0))
print(int("z", base=36))
print(int("0b101", 0), int("0xff", 0))
print(int("  42  ", base=10))
print(int("-1010", base=2))
try:
    int(x="123")
except TypeError:
    print("TE x-kwarg")
try:
    int("xyz", base=2)
except ValueError as e:
    print("VE")
try:
    int(255, base=16)
except TypeError:
    print("TE non-string base")
