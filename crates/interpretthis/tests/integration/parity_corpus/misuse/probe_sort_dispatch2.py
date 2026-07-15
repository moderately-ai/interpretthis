class Card:
    def __init__(self, rank): self.rank = rank
    def __lt__(self, o): return self.rank < o.rank
    def __repr__(self): return f"C{self.rank}"
cards = [Card(5), Card(2), Card(8), Card(1)]
cards.sort()
print(cards)
cards.sort(reverse=True)
print(cards)
print(sorted(cards, key=lambda c: -c.rank))
from functools import reduce
print(reduce(lambda a, b: a if a.rank > b.rank else b, cards))
print(max(cards, key=lambda c: c.rank))
nested = [[Card(3)], [Card(1)], [Card(2)]]
print(sorted(nested, key=lambda x: x[0]))
print(min(cards).rank)
groups = {}
for c in cards:
    groups.setdefault(c.rank % 2, []).append(c)
print({k: len(v) for k, v in groups.items()})
