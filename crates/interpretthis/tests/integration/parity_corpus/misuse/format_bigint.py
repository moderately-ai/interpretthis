print(f"{2**100:x}")
print(f"{2**64:b}")
print(format(255, "x"))
try:
    print(f"{5:q}")
except ValueError as e:
    print("badcode:", type(e).__name__)
try:
    print(f"{True:d}")
    print(f"{True:d}" == "1")
except Exception as e:
    print(type(e).__name__)
