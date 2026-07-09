# Pins: user-class __format__ dispatches when an f-string applies a
# format spec (`f"{val:.2f}"`, `f"{val:>10}"`, `f"{val:hex}"`).
# Customer pattern: domain types (Money, Date) with controlled
# string rendering via format-spec.
class Money:
    def __init__(self, cents):
        self.cents = cents
    def __format__(self, spec):
        amount = self.cents / 100
        if spec == "":
            return f"${amount:.2f}"
        if spec == "k":
            return f"${amount / 1000:.1f}k"
        return f"${amount:{spec}}"

m = Money(150000)
print(f"{m}")
print(f"{m:k}")
print(f"{m:.0f}")
print(format(m))
print(format(m, "k"))
