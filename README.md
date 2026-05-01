# Netswitch 🛜

Netswitch is a lightweight, cross-platform utility designed to keep you connected. It monitors your network interfaces and automatically performs a seamless switchover if your primary internet connection fails.

> **Why Netswitch?**
> I built this project because I needed a simple, reliable solution that does one thing well: internet failover. I wanted something that just works in the background without complex configuration, ensuring my workflow isn't interrupted when a connection drops.

---

## ✨ Features

- **🚀 Automatic Failover**: Switches to the next available interface (Wi-Fi, Ethernet, etc.) the moment internet connectivity is lost.
- **📈 Live Dashboard**: View the real-time status and health of all your network adapters.
- **🖱️ Drag-to-Prioritize**: Simple drag-and-drop interface to set your preferred network order.
- **🔒 Set & Forget**: Once set up, it runs as a lightweight background service.

---

## 📥 Installation

1.  **Download**: Get the latest installer for your operating system (macOS, Windows, or Linux) from the [Releases](https://github.com/optimumsage/netswitch/releases) page.
2.  **Install**: Open the installer and follow the standard installation steps.
3.  **Setup Daemon**: On first launch, the app will ask for permission to start the background service (the "Daemon"). This is required to manage your network routing.

---

## 🛠️ Usage

- **Priority**: The interface at the top of the list is your primary choice. If it has internet, Netswitch will use it.
- **Reordering**: Drag any interface to the top to make it your new primary connection.
- **Status Symbols**:
  - 🟢 **Green**: Connected to the internet.
  - 🔴 **Red**: Interface is active but has no internet access.
  - 🔵 **Primary Badge**: This is your current active route to the world.

---

## 🤝 Contributing

Netswitch is an open-source project. Contributions, bug reports, and feature requests are welcome!

---

## 📄 License

Licensed under the **PolyForm Noncommercial License 1.0.0**. Free for individuals and non-commercial use. See the [LICENSE](LICENSE) file for details.

**Developed with ❤️ by [Optimum Sage](https://github.com/optimumsage)**
