class Money:
    def __init__(self, amount):
        self.amount = amount
    def __add__(self, o):
        return Money(self.amount + (o.amount if isinstance(o, Money) else o))
    def __radd__(self, o):
        return Money(o + self.amount)
    def __mul__(self, n):
        return Money(self.amount * n)
    def __rmul__(self, n):
        return Money(n * self.amount)
    def __repr__(self):
        return f"Money({self.amount})"
print(Money(10) + 5)
print(5 + Money(10))
print(Money(10) * 3)
print(3 * Money(10))
print(sum([Money(1), Money(2), Money(3)]))
class Temp:
    def __init__(self, c):
        self.c = c
    def __lt__(self, o):
        return self.c < o.c
    def __le__(self, o):
        return self.c <= o.c
    def __eq__(self, o):
        return self.c == o.c
    def __repr__(self):
        return f"{self.c}C"
temps = [Temp(20), Temp(10), Temp(30)]
print(sorted(temps))
print(min(temps), max(temps))
print(Temp(10) < Temp(20))
