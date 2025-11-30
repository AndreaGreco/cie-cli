//! Comando per elencare i lettori di smart card

use anyhow::Result;
use super::create_cie;

pub fn execute(pkcs11_lib: Option<&str>) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;
    let readers = cie.list_readers()?;

    if readers.is_empty() {
        println!("Nessun lettore di smart card trovato.");
        println!("\nAssicurati che:");
        println!("  - Il lettore sia collegato");
        println!("  - I driver pcscd siano installati e attivi");
        println!("  - La libreria CIE PKCS#11 sia installata");
        return Ok(());
    }

    println!("Lettori trovati: {}\n", readers.len());
    
    for (i, reader) in readers.iter().enumerate() {
        println!("{}. {}", i + 1, reader.slot_info.description);
        println!("   Produttore: {}", reader.slot_info.manufacturer);
        println!("   HW: {} | FW: {}", 
            reader.slot_info.hardware_version,
            reader.slot_info.firmware_version
        );
        
        if let Some(ref token) = reader.token_info {
            println!("   ✓ CIE inserita:");
            println!("     - Label: {}", token.label);
            println!("     - Modello: {}", token.model);
            println!("     - S/N: {}", token.serial_number);
        } else {
            println!("   ✗ Nessuna CIE inserita");
        }
        println!();
    }

    Ok(())
}
