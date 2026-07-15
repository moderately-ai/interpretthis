from dataclasses import dataclass
@dataclass
class Base:
    x: int
    y: int = 0
@dataclass
class Derived(Base):
    z: int = 5
d = Derived(1, 2, 3)
print(d.x, d.y, d.z)
print(d)
d2 = Derived(10)
print(d2.x, d2.y, d2.z)
@dataclass
class Point3D:
    x: float
    y: float
    z: float
    def magnitude(self):
        return (self.x**2 + self.y**2 + self.z**2) ** 0.5
p = Point3D(3, 4, 0)
print(p.magnitude())
print(p == Point3D(3, 4, 0))
print(p != Point3D(1, 1, 1))
@dataclass
class Container:
    name: str
    items: list
c = Container("box", [1, 2, 3])
print(c.name, c.items)
