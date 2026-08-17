"""Helper module — has long param list and deep nesting."""


def do_everything(a, b, c, d, e, f, g):
    """Too many parameters."""
    if a:
        if b:
            if c:
                if d:
                    if e:
                        return f + g
    return 0
