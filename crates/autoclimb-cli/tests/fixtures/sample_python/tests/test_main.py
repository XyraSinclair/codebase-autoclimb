"""Tests for main module."""

from src.main import process


def test_process():
    assert process('{"a": 1}') == {"a": 1}
