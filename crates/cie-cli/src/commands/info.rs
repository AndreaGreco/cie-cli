//! Comando per mostrare informazioni sulla CIE

use anyhow::Result;
use super::create_cie;

pub fn execute(pkcs11_lib: Option<&str>) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;
    
    let reader = match cie.find_cie() {
        Ok(r) => r,
        Err(cie_lib::CieError::NoCiePresent) => {
            println!("Nessuna CIE trovata.");
            println!("\nInserisci la CIE nel lettore e riprova.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    println!("CIE trovata!\n");
    
    println!("Lettore: {}", reader.slot_info.description);
    
    if let Some(ref token) = reader.token_info {
        println!("\n╔══════════════════════════════════════╗");
        println!("║     CARTA D'IDENTITÀ ELETTRONICA     ║");
        println!("╠══════════════════════════════════════╣");
        println!("║ Label:    {:<25} ║", token.label);
        println!("║ Modello:  {:<25} ║", token.model);
        println!("║ Seriale:  {:<25} ║", token.serial_number);
        println!("║ Prodotto: {:<25} ║", token.manufacturer);
        println!("╚══════════════════════════════════════╝");
    }

    println!("\nUsa 'cie cert list' per vedere i certificati.");
    println!("Usa 'cie sign -i <file>' per firmare un documento.");

    Ok(())
}
