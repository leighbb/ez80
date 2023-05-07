use super::machine::*;
use super::registers::*;
use super::state::State;

pub struct Environment<'a> {
    pub state: &'a mut State,
    pub sys: &'a mut dyn Machine
}

impl <'a> Environment<'_> {
    pub fn new(state: &'a mut State, sys: &'a mut dyn Machine) -> Environment<'a> {
        Environment {
            state,
            sys
        }
    }

    pub fn wrap_address(&self, address: u32, increment: i32) -> u32 {
        if self.state.is_op_long() {
            address.wrapping_add(increment as u32)
        } else {
            (address & 0xff0000) + (address as u16).wrapping_add(increment as u16) as u32
        }
    }

    pub fn peek(&self, address: u32) -> u8 {
        self.sys.peek(address)
    }

    /// Sets the memory content to [value] in [address]
    pub fn poke(&mut self, address: u32, value: u8) {
        self.sys.poke(address, value);
    }

    /// Returns the memory contents in [address] as word
    pub fn peek16(&self, address: u32) -> u16 {
        self.sys.peek(address) as u16
        + ((self.sys.peek(self.wrap_address(address, 1)) as u16) << 8)
    }

    /// Sets the memory content to the word [value] in [address]
    pub fn poke16(&mut self, address: u32, value: u16) {
        self.sys.poke(address, value as u8 );
        self.sys.poke(self.wrap_address(address, 1), (value >> 8) as u8);
    }

    pub fn peek24(&self, address: u32) -> u32 {
        self.sys.peek(address) as u32
        + ((self.sys.peek(self.wrap_address(address, 1)) as u32) << 8)
        + ((self.sys.peek(self.wrap_address(address, 2)) as u32) << 16)
    }

    pub fn poke24(&mut self, address: u32, value: u32) {
        self.sys.poke(address, value as u8 );
        self.sys.poke(self.wrap_address(address, 1), (value >> 8) as u8);
        self.sys.poke(self.wrap_address(address, 2), (value >> 16) as u8);
    }

    pub fn peek_pc(&self) -> u8 {
        let pc = self.state.pc();
        self.sys.peek(pc)
    }

    pub fn advance_pc(&mut self) -> u8 {
        let pc = self.state.pc();
        let value = self.sys.peek(pc);
        self.state.set_pc(pc.wrapping_add(1));
        value
    }

    pub fn peek16_pc(&self) -> u16 {
        let pc = self.state.pc();
        self.peek16(pc)
    }

    pub fn advance_immediate16(&mut self) -> u16 {
        let mut value: u16 = self.advance_pc() as u16;
        value += (self.advance_pc() as u16) << 8;
        value
    }

    pub fn advance_immediate24(&mut self) -> u32 {
        let mut value = self.advance_pc() as u32;
        value += (self.advance_pc() as u32) << 8;
        value += (self.advance_pc() as u32) << 16;
        value
    }

    pub fn advance_immediate16or24(&mut self) -> u32 {
        if self.state.is_imm_long() {
            self.advance_immediate24()
        } else {
            self.advance_immediate16() as u32
        }
    }

    pub fn advance_immediate_16mbase_or_24(&mut self) -> u32 {
        if self.state.is_imm_long() {
            self.advance_immediate24()
        } else {
            ((self.state.reg.mbase as u32) << 16) + self.advance_immediate16() as u32
        }
    }

    pub fn push(&mut self, value: u32) {
        let mut sp = self.state.sp();

        let u = (value >> 16) as u8;
        let h = (value >> 8) as u8;
        let l = value as u8;

        if self.state.is_op_long() {
            sp = sp.wrapping_sub(1);
            self.sys.poke(sp, u);
        }

        sp = sp.wrapping_sub(1);
        self.sys.poke(sp, h);

        sp = sp.wrapping_sub(1);
        self.sys.poke(sp, l);

        if self.state.is_op_long() {
            self.state.reg.set24(Reg16::SP, sp);
        } else {
            self.state.reg.set16(Reg16::SP, sp as u16);
        }
    }

    pub fn pop(&mut self) -> u32 {
        let mut sp = self.state.sp();

        let mut u = 0;

        let l = self.sys.peek(sp);
        sp = self.wrap_address(sp, 1);

        let h = self.sys.peek(sp);
        sp = self.wrap_address(sp, 1);

        if self.state.is_op_long() {
            u = self.sys.peek(sp);
            sp = self.wrap_address(sp, 1);
        }

        if self.state.is_op_long() {
            self.state.reg.set24(Reg16::SP, sp);
        } else {
            self.state.reg.set16(Reg16::SP, sp as u16);
        }
        (l as u32) + ((h as u32) << 8) + ((u as u32) << 16)
    }

    pub fn subroutine_call(&mut self, address: u32) {
        println!("CALL ${:04x}", address);
        self.push(self.state.pc());
        self.state.set_pc(address);
    }

    pub fn subroutine_return(&mut self) {
        //println!("RETURN");
        let pc = self.pop();
        if pc == 0 { panic!("reset!") };
        self.state.set_pc(pc);
    }

    pub fn set_index(&mut self, index: Reg16) {
        self.state.index = index;
    }

    pub fn clear_index(&mut self) {
        self.state.index = Reg16::HL;
    }

    pub fn get_index(&self) -> Reg16 {
        self.state.index
    }

    pub fn index_description(&self) -> String {
        if self.state.index == Reg16::HL {
            "HL".to_string()
        } else {
            format!("{:?}{:+}", self.state.index, self.state.displacement)
        }
    }

    pub fn is_alt_index(& self) -> bool {
        self.state.index != Reg16::HL
    }

    pub fn load_displacement(&mut self) {
        /*
        The displacement byte is a signed 8-bit integer (-128..+127) used
        in some instructions to specify a displacement added to a given
        memory address. Its presence or absence depends on the instruction
        at hand, therefore, after reading the prefix and opcode, one has
        enough information to figure out whether to expect a displacement
        byte or not.
        */
        self.state.displacement = self.advance_pc() as i8;
    }

    pub fn index_value(& self) -> u32 {
        if self.state.is_op_long() {
            self.state.reg.get24(self.state.index)
        } else {
            self.state.reg.get16_mbase(self.state.index)
        }
    }

    pub fn index_address(&self) -> u32 {
        // Pseudo register (HL), (IX+d), (IY+d)
        let address = if self.state.is_op_long() {
            self.state.reg.get16_mbase(self.state.index)
        } else {
            self.state.reg.get24(self.state.index)
        };
        if self.is_alt_index() {
            (address as i32).wrapping_add(self.state.displacement as i32) as u32
        } else {
            address
        }
    }

    fn translate_reg(&self, reg: Reg8) -> Reg8 {
        match self.state.index {
            Reg16::IX => match reg {
                Reg8::H => Reg8::IXH,
                Reg8::L => Reg8::IXL,
                _ => reg
            },
            Reg16::IY => match reg {
                Reg8::H => Reg8::IYH,
                Reg8::L => Reg8::IYL,
                _ => reg
            },
            _ => reg
        }
    }

    pub fn reg8_ext(& self, reg: Reg8) -> u8 {
        if reg == Reg8::_HL {
            self.sys.peek(self.index_address())
        } else {
            self.state.reg.get8(self.translate_reg(reg))
        }
    }

    pub fn reg16mbase_or_24(&mut self, rr: Reg16) -> u32 {
        if self.state.is_op_long() {
            self.state.reg.get24(rr)
        } else {
            self.state.reg.get16_mbase(rr)
        }
    }

    pub fn reg16or24_ext(& self, rr: Reg16) -> u32 {
        if self.state.is_op_long() {
            if rr == Reg16::HL {
                self.state.reg.get24(self.state.index)
            } else if rr == Reg16::AF {
                self.state.reg.get16(rr) as u32
            } else {
                self.state.reg.get24(rr)
            }
        } else {
            if rr == Reg16::HL {
                self.state.reg.get16(self.state.index) as u32
            } else {
                self.state.reg.get16(rr) as u32
            }
        }
    }

    pub fn set_reg(&mut self, reg: Reg8, value: u8) {
        if reg == Reg8::_HL {
            self.sys.poke(self.index_address(), value);
        } else {
            self.state.reg.set8(self.translate_reg(reg), value);
        }
    }

    pub fn set_reg16or24(&mut self, rr: Reg16, value: u32) {
        if self.state.is_op_long() {
            self.set_reg24(rr, value);
        } else {
            self.set_reg16(rr, value as u16);
        }
    }

    pub fn set_reg16(&mut self, rr: Reg16, value: u16) {
        if rr == Reg16::HL {
            self.state.reg.set16(self.state.index, value);
        } else {
            self.state.reg.set16(rr, value);
        }
    }

    pub fn set_reg24(&mut self, rr: Reg16, value: u32) {
        if rr == Reg16::HL {
            self.state.reg.set24(self.state.index, value);
        } else {
            self.state.reg.set24(rr, value);
        }
    }

    pub fn port_in(&mut self, address: u16) -> u8 {
        self.sys.port_in(address)
    }

    pub fn port_out(&mut self, address: u16, value: u8) {
        self.sys.port_out(address, value);
    }
}
