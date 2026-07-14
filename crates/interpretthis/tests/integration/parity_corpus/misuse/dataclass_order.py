from dataclasses import dataclass, field
@dataclass(order=True)
class P:
    x: int
    y: int
print(P(1, 2) < P(1, 3))
print(P(1, 2) == P(1, 2))
print(sorted([P(2, 1), P(1, 9), P(1, 2)]))
