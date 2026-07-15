from abc import ABC, abstractmethod


class Shape(ABC):
    @abstractmethod
    def area(self):
        pass

    def describe(self):
        return f"A shape with area {self.area()}"


class Circle(Shape):
    def __init__(self, r):
        self.r = r

    def area(self):
        return round(3.14159 * self.r ** 2, 2)


c = Circle(2)
print(c.area(), c.describe())
print(isinstance(c, Shape), isinstance(c, Circle))
try:
    Shape()
except TypeError as e:
    print("Shape abstract:", "abstract" in str(e), "area" in str(e))


class Base(ABC):
    @abstractmethod
    def foo(self):
        ...

    @abstractmethod
    def bar(self):
        ...


try:
    Base()
except TypeError as e:
    print("two:", "foo" in str(e) and "bar" in str(e))


class Partial(Base):
    def foo(self):
        return 1


try:
    Partial()
except TypeError as e:
    print("partial still abstract:", "bar" in str(e) and "foo" not in str(e))


class Complete(Base):
    def foo(self):
        return 1

    def bar(self):
        return 2


p = Complete()
print(p.foo(), p.bar())

import abc


class Widget(abc.ABC):
    @abc.abstractmethod
    def render(self):
        pass


class Button(Widget):
    def render(self):
        return "button"


print(Button().render())


# A grandchild that leaves an abstract method unimplemented stays abstract.
class Mid(Shape):
    def helper(self):
        return 1


try:
    Mid()
except TypeError as e:
    print("mid abstract:", "area" in str(e))
