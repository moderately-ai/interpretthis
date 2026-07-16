from abc import ABCMeta, abstractmethod


# A class can use metaclass=ABCMeta directly (not only class C(ABC)); the
# abstract-method tracking still makes it uninstantiable until implemented.
class Base(metaclass=ABCMeta):
    @abstractmethod
    def run(self): ...


class Impl(Base):
    def run(self):
        return "running"


print(Impl().run())
print(isinstance(Impl(), Base))
print(issubclass(Impl, Base))

try:
    Base()
except TypeError as e:
    print("abstract:", type(e).__name__)


class Incomplete(Base):
    pass


try:
    Incomplete()
except TypeError as e:
    print("incomplete:", type(e).__name__)


# Abstract property through ABCMeta.
class Container(metaclass=ABCMeta):
    @property
    @abstractmethod
    def size(self): ...


class Box(Container):
    @property
    def size(self):
        return 10


print(Box().size)
