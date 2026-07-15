class Money:
    def __init__(self, amount):
        self.amount = amount
    def __format__(self, spec):
        if spec == "":
            return f"${self.amount}"
        return f"${self.amount:{spec}}"
    def __str__(self):
        return f"${self.amount}"
m = Money(1234.5)
print(f"{m}")
print(f"{m:.2f}")
print(format(m, ",.2f"))
print("{}".format(m))
print("{:.1f}".format(m))
