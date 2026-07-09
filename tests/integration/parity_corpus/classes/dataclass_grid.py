# Pins: @dataclass with default + default_factory; repr; method
# definitions; structural equality.
from dataclasses import dataclass, field

@dataclass
class Item:
    name: str
    qty: int = 1
    tags: list = field(default_factory=list)
    def total(self, price):
        return self.qty * price

i = Item("widget", 3)
print(i)
print(i.total(10))
i.tags.append("popular")
print(i)

@dataclass
class Box:
    w: int
    h: int

print(Box(1, 2) == Box(1, 2))
print(Box(1, 2) == Box(1, 3))
