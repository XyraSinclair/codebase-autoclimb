"""Main module — has unused imports and a code smell."""

from os.path import join, exists  # exists is unused
import json


items = []  # mutable global


def process(data):
    """Process data."""
    path = join("a", "b")
    return json.loads(data)
