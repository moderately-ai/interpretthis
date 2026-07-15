from dataclasses import dataclass, field, asdict, astuple, replace
@dataclass
class Point:
    x: int
    y: int
p = Point(1, 2)
print(asdict(p))
print(astuple(p))
q = replace(p, x=10)
print(q)
@dataclass(frozen=True, order=True)
class Version:
    major: int
    minor: int
    patch: int = 0
v1 = Version(1, 2, 3)
v2 = Version(1, 3)
print(v1 < v2)
print(sorted([Version(2, 0), Version(1, 5), Version(1, 0)]))
print(v1)
print(hash(v1) == hash(Version(1, 2, 3)))
@dataclass
class Container:
    items: list = field(default_factory=list)
    name: str = "default"
c = Container()
c.items.append(1)
print(c.items, c.name)
