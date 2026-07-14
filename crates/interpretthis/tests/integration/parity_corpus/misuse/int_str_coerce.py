print(int("1_000"))
print(int("0xff", 16))
print(int("  42  "))
try:
    print(int("1.5"))
except ValueError as e:
    print("float-str:", type(e).__name__)
try:
    print(int(3.9))
except Exception as e:
    print(type(e).__name__)
print(int(3.9))
