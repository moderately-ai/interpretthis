# issubclass walks a user exception subclass's stored MRO AND expands the
# builtin-exception parent chain past the first builtin base, so
# issubclass(ValidationError, Exception) is True even though the stored MRO is
# only [ValidationError, ValueError].
class MyErr(Exception):
    pass


class ValErr(ValueError):
    pass


class DeepErr(ValErr):
    pass


class OSErr(OSError):
    pass


class ConnErr(ConnectionError):
    pass


print(issubclass(MyErr, Exception), issubclass(MyErr, BaseException))
print(issubclass(ValErr, ValueError), issubclass(ValErr, Exception), issubclass(ValErr, BaseException))
print(issubclass(DeepErr, ValErr), issubclass(DeepErr, ValueError), issubclass(DeepErr, Exception))
print(issubclass(OSErr, OSError), issubclass(OSErr, Exception))
print(issubclass(ConnErr, ConnectionError), issubclass(ConnErr, OSError), issubclass(ConnErr, Exception))


class Plain:
    pass


print(issubclass(Plain, Exception), issubclass(Plain, object))
print(issubclass(ValErr, (TypeError, ValueError)), issubclass(MyErr, (TypeError, KeyError)))
print(isinstance(ValErr("x"), Exception), isinstance(DeepErr("y"), ValueError))
