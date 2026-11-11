#!/usr/bin/env python3
"""A minimal org.kde.StatusNotifierWatcher for the x11_single_instance step.

Run it on a PRIVATE session bus (dbus-run-session), never the desktop's: it
takes the watcher name, so tray items register with it instead of a bar.
Like Waybar's watcher it drops an item when the bus name that registered it
goes away (an app that quits or is killed). `--no-prune` keeps them, as a
naive tray host would. The registered list is the watcher's
RegisteredStatusNotifierItems property; the step reads it with gdbus.

usage: sni_watcher.py [--no-prune]
"""
import sys

from gi.repository import Gio, GLib

PRUNE = "--no-prune" not in sys.argv[1:]
XML = """<node><interface name="org.kde.StatusNotifierWatcher">
<method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
<method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
<property name="RegisteredStatusNotifierItems" type="as" access="read"/>
<property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
<property name="ProtocolVersion" type="i" access="read"/>
<signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
<signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
</interface></node>"""

items = []  # (item, owner): Rune registers its connection's unique name
watches = {}


def log(message):
    print(message, flush=True)


def on_call(conn, sender, path, iface, method, params, invocation):
    if method == "RegisterStatusNotifierItem":
        service = params.unpack()[0]
        item = service if service.startswith(":") else f"{sender}{service}"
        if item not in [i for i, _ in items]:
            items.append((item, sender))
            log(f"registered {item}")
            if PRUNE and sender not in watches:
                watches[sender] = Gio.bus_watch_name_on_connection(
                    conn, sender, Gio.BusNameWatcherFlags.NONE, None, lambda c, name: vanished(name)
                )
    invocation.return_value(None)


def vanished(name):
    global items
    gone = [i for i, owner in items if owner == name]
    items = [(i, owner) for i, owner in items if owner != name]
    for item in gone:
        log(f"unregistered {item}")


def on_get(conn, sender, path, iface, prop):
    if prop == "RegisteredStatusNotifierItems":
        return GLib.Variant("as", [i for i, _ in items])
    if prop == "IsStatusNotifierHostRegistered":
        return GLib.Variant("b", True)
    return GLib.Variant("i", 0)


node = Gio.DBusNodeInfo.new_for_xml(XML)


def acquired(conn, name):
    conn.register_object("/StatusNotifierWatcher", node.interfaces[0], on_call, on_get, None)


Gio.bus_own_name(
    Gio.BusType.SESSION,
    "org.kde.StatusNotifierWatcher",
    Gio.BusNameOwnerFlags.NONE,
    acquired,
    lambda conn, name: log("watching"),
    lambda conn, name: sys.exit("lost the watcher name"),
)
GLib.MainLoop().run()
