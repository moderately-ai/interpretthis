from dataclasses import dataclass, field, InitVar, fields, asdict
from typing import ClassVar
@dataclass
class Config:
    name: str
    count: int = 0
    tags: list = field(default_factory=list)
    _cache: dict = field(default_factory=dict, repr=False)
    version: ClassVar[str] = "1.0"
c = Config("test", 5)
print(c)
print(Config.version)
print([f.name for f in fields(c)])
@dataclass
class Point:
    x: int
    y: int
    total: int = field(init=False, default=0)
    def __post_init__(self):
        self.total = self.x + self.y
p = Point(3, 4)
print(p.total)
@dataclass
class WithInit:
    value: int
    multiplier: InitVar[int] = 2
    result: int = field(init=False)
    def __post_init__(self, multiplier):
        self.result = self.value * multiplier
w = WithInit(5, 3)
print(w.result)
print(hasattr(w, "multiplier"))
@dataclass(frozen=True, eq=True)
class Frozen:
    a: int
    b: int
f1, f2 = Frozen(1, 2), Frozen(1, 2)
print(f1 == f2, hash(f1) == hash(f2))
print({f1, f2})
@dataclass(order=True)
class Version:
    major: int
    minor: int
    patch: int = 0
print(sorted([Version(1, 2, 0), Version(1, 0, 5), Version(2, 0)]))
print(Version(1, 0) < Version(1, 1))
print(asdict(Config("x", 1, ["a", "b"])))
