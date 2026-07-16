from dataclasses import dataclass, field, fields, asdict, astuple, replace
@dataclass
class Point:
    x: int
    y: int
p = Point(1, 2)
print(p.x, p.y)
print(p)
print(p == Point(1, 2))
print(p == Point(1, 3))
@dataclass
class Config:
    name: str
    count: int = 0
    tags: list = field(default_factory=list)
c = Config("test")
print(c.name, c.count, c.tags)
c.tags.append("a")
print(c.tags)
c2 = Config("test")
print(c2.tags)
@dataclass(frozen=True)
class Immutable:
    value: int
i = Immutable(42)
print(i.value)
try:
    i.value = 100
except Exception as e:
    print(type(e).__name__)
@dataclass(order=True)
class Version:
    major: int
    minor: int
print(Version(1, 2) < Version(1, 3))
print(Version(2, 0) > Version(1, 9))
print(sorted([Version(2, 1), Version(1, 5), Version(2, 0)]))
@dataclass
class WithPost:
    a: int
    b: int
    total: int = 0
    def __post_init__(self):
        self.total = self.a + self.b
wp = WithPost(3, 4)
print(wp.total)
@dataclass
class Person:
    name: str
    age: int
person = Person("Alice", 30)
print(asdict(person))
print(astuple(person))
print([f.name for f in fields(person)])
p2 = replace(person, age=31)
print(p2.name, p2.age, person.age)
@dataclass
class Base:
    a: int
@dataclass
class Derived(Base):
    b: int
d = Derived(1, 2)
print(d.a, d.b)
print(d)
@dataclass
class Defaults:
    x: int = 1
    y: int = 2
    z: int = 3
print(Defaults())
print(Defaults(10))
print(Defaults(z=30))
@dataclass
class Repr:
    val: int
print(repr(Repr(5)))
