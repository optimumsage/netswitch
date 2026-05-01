fn main() {
    let interfaces = default_net::get_interfaces();
    println!("Total interfaces found: {}", interfaces.len());
    for iface in interfaces {
        println!("Name: {}, Friendly: {:?}, IPv4: {:?}, Loopback: {}", 
            iface.name, 
            iface.friendly_name, 
            iface.ipv4, 
            iface.is_loopback()
        );
    }
}
