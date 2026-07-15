class Money:
    def __init__(self, c): self.c = c
    def __format__(self, spec): return f"${self.c/100:{spec or '.2f'}}"
print(format(Money(12345)))
print(format(Money(12345), ".1f"))
print(f"{Money(500)}")
print(format(255, "x"), format(255, "#x"), format(3.14159, ".2f"))
print(format(1000000, ","), format(0.5, "%"))
print("{:>10}".format("hi"), "{:.3f}".format(3.14159))
print(format(42, "b"), format(65, "c"))
class Idx:
    def __index__(self): return 10
print(format(Idx(), "x"))
