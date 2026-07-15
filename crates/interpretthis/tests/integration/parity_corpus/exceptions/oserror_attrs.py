try:
    raise OSError(2, "No such file")
except OSError as e:
    print(e.errno, e.strerror, e.args)
try:
    raise FileNotFoundError(2, "not found", "/path")
except OSError as e:
    print(e.errno, e.strerror, e.filename)
try:
    raise OSError("just a message")
except OSError as e:
    print(e.errno, e.strerror, e.args)
try:
    raise PermissionError(13, "Permission denied")
except OSError as e:
    print(e.errno, e.strerror, isinstance(e, OSError))
try:
    raise ConnectionError(111, "Connection refused")
except OSError as e:
    print(e.errno, e.strerror)
