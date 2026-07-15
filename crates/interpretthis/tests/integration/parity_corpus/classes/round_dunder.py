class Money:
    def __init__(self, cents): self.cents = cents
    def __round__(self, n=0): return round(self.cents / 100, n)
    def __repr__(self): return f"${self.cents / 100:.2f}"
m = Money(1050)
print(round(m))
print(round(m, 1))
print(round(Money(1256), 1))
class Approx:
    def __round__(self, ndigits=None):
        return "no-arg" if ndigits is None else f"arg={ndigits}"
a = Approx()
print(round(a))
print(round(a, 3))
class NoRound:
    pass
try:
    round(NoRound())
except TypeError as e:
    print("TypeError")
